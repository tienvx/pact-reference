//! Collection of utilities for working with XML

use std::collections::BTreeMap;
use std::ops::Index;
use std::str;

use anyhow::anyhow;
use indextree::{Arena, NodeId};
use itertools::Itertools;
use kiss_xml::dom::{Element, Node};
use sxd_document::{Package, parser};
use tracing::trace;

use crate::path_exp::{DocPath, PathToken};

/// Parses a vector of bytes into a XML document
pub fn parse_bytes(bytes: &[u8]) -> anyhow::Result<Package> {
  let string = str::from_utf8(bytes)?;
  match parser::parse(string) {
    Ok(doc) => Ok(doc),
    Err(err) => Err(anyhow!("Failed to parse bytes as XML - {}", err))
  }
}

/// Resolve the path expression against the XML, returning a list of pointer values that match.
pub fn resolve_path(value: &Element, expression: &DocPath) -> Vec<String> {
  let mut tree = Arena::new();
  let root = tree.new_node("".into());

  let tokens = expression.tokens();
  query_graph(tokens.as_slice(), &mut tree, root, value, 0);

  let tokens = expression.tokens().iter()
    .filter(|t| match t {
      PathToken::Index(_) => false,
      _ => true
    }).collect_vec();
  let expanded_paths = root.descendants(&tree)
    .fold(Vec::<String>::new(), |mut acc, node_id| {
    let node = tree.index(node_id);
    if !node.get().is_empty() && node.first_child().is_none() {
      let path: Vec<String> = node_id.ancestors(&tree)
        .map(|n| format!("{}", tree.index(n).get()))
        .collect();
      if path.len() == tokens.len() {
        acc.push(path.iter().rev().join("/"));
      }
    }
    acc
  });
  expanded_paths
}

fn query_graph(
  path_iter: &[PathToken],
  tree: &mut Arena<String>,
  parent_id: NodeId,
  element: &Element,
  index: usize
) {
  trace!(?path_iter, %parent_id, index, %element, ">>> query_graph");

  if let Some(token) = path_iter.first() {
    trace!(?token, "next token");
    match token {
      PathToken::Field(name) => {
        if element.name() == name.as_str() {
          trace!(name, %parent_id, "Field name matches element");
          let node_id = parent_id.append_value(format!("{}[{}]", name, index), tree);

          let remaining_tokens = &path_iter[1..];
          if !remaining_tokens.is_empty() {
            query_attributes(remaining_tokens, tree, node_id, element, index);
            query_text(remaining_tokens, tree, node_id, element, index);

            if let Some(PathToken::Index(i)) = remaining_tokens.first() {
              query_graph(remaining_tokens, tree, node_id, element, index);
            }

            let grouped_children = group_children(element);
            for children in grouped_children.values() {
              for (index, child) in children.iter().enumerate() {
                query_graph(remaining_tokens, tree, node_id, *child, index);
              }
            }
          }
        }
      },
      PathToken::Index(i) => {
        if *i == index {
          let remaining_tokens = &path_iter[1..];
          if !remaining_tokens.is_empty() {
            query_attributes(remaining_tokens, tree, parent_id, element, index);
            query_text(remaining_tokens, tree, parent_id, element, index);

            let grouped_children = group_children(element);
            for (_, children) in grouped_children {
              for (index, child) in children.iter().enumerate() {
                query_graph(remaining_tokens, tree, parent_id, *child, index);
              }
            }
          }
        }
      }
      PathToken::Star | PathToken::StarIndex => {
        trace!(%parent_id, name = element.name(), "* -> Adding current node to parent");
        let node_id = parent_id.append_value(format!("{}[{}]", element.name(), index), tree);

        let remaining_tokens = &path_iter[1..];
        if !remaining_tokens.is_empty() {
          query_attributes(remaining_tokens, tree, node_id, element, index);
          query_text(remaining_tokens, tree, node_id, element, index);

          let grouped_children = group_children(element);
          for (_, children) in grouped_children {
            for (index, child) in children.iter().enumerate() {
              query_graph(remaining_tokens, tree, node_id, *child, index);
            }
          }
        }
      },
      PathToken::Root => {
        query_graph(&path_iter[1..], tree, parent_id, element, index);
      }
    }
  }
}

fn group_children(element: &Element) -> BTreeMap<String, Vec<&Element>> {
  element.child_elements()
    .fold(BTreeMap::new(), |mut acc, child| {
      acc.entry(child.name())
        .and_modify(|entry: &mut Vec<_>| entry.push(child))
        .or_insert_with(|| vec![child]);
      acc
    })
}

fn query_attributes(
  path_iter: &[PathToken],
  tree: &mut Arena<String>,
  parent_id: NodeId,
  element: &Element,
  index: usize
) {
  trace!(?path_iter, %parent_id, index, %element, ">>> query_attributes");

  if let Some(token) = path_iter.first() {
    trace!(?token, "next token");
    if let PathToken::Field(name) = token {
      if name.starts_with('@') {
        let attribute_name = &name[1..];
        if element.attributes().contains_key(attribute_name) {
          trace!(name, "Field name matches element attribute");
          parent_id.append_value(name.clone(), tree);
        }
      }
    }
  }
}

fn query_text(
  path_iter: &[PathToken],
  tree: &mut Arena<String>,
  parent_id: NodeId,
  element: &Element,
  index: usize
) {
  trace!(?path_iter, %parent_id, index, %element, ">>> query_text");

  if let Some(token) = path_iter.first() {
    trace!(?token, "next token");
    if let PathToken::Field(name) = token {
      if name == "#text" && !element.text().is_empty() {
        trace!(name, "Field name matches element text");
        parent_id.append_value(name.clone(), tree);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use expectest::prelude::*;

  use crate::path_exp::DocPath;

  use super::*;

  #[test_log::test]
  fn basic_xml_test() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
      <config>
        <name>My Settings</name>
        <sound>
          <property name="volume" value="11" />
          <property name="mixer" value="standard" />
        </sound>
      </config>
      "#;
    let dom = kiss_xml::parse_str(xml).unwrap();
    let root = dom.root_element();

    let path = DocPath::root();
    expect!(resolve_path(root, &path).is_empty()).to(be_true());

    let path = DocPath::new_unwrap("$.config");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec!["/config[0]"]));

    let path = DocPath::new_unwrap("$.config.sound");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec!["/config[0]/sound[0]"]));

    let path = DocPath::new_unwrap("$.config.sound.property");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/sound[0]/property[0]",
      "/config[0]/sound[0]/property[1]"
    ]));

    let path = DocPath::new_unwrap("$.config.*");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/name[0]",
      "/config[0]/sound[0]"
    ]));

    let path = DocPath::new_unwrap("$.config[*]");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/name[0]",
      "/config[0]/sound[0]"
    ]));

    let path = DocPath::new_unwrap("$.config.sound.property.@name");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/sound[0]/property[0]/@name",
      "/config[0]/sound[0]/property[1]/@name"
    ]));

    let path = DocPath::new_unwrap("$.config.sound.property.@other");
    expect!(resolve_path(root, &path).is_empty()).to(be_true());

    let path = DocPath::new_unwrap("$.config.sound.*.@name");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/sound[0]/property[0]/@name",
      "/config[0]/sound[0]/property[1]/@name"
    ]));

    let path = DocPath::new_unwrap("$.config.name.#text");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec!["/config[0]/name[0]/#text"]));

    let path = DocPath::new_unwrap("$.config.*.#text");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec!["/config[0]/name[0]/#text"]));

    let path = DocPath::new_unwrap("$.config.sound.property.#text");
    expect!(resolve_path(root, &path).is_empty()).to(be_true());

    let path = DocPath::new_unwrap("$.config.sound.property[1].@name");
    expect!(resolve_path(root, &path)).to(be_equal_to(vec![
      "/config[0]/sound[0]/property[1]/@name"
    ]));

    let path = DocPath::new_unwrap("$.config.sound.property[2].@name");
    expect!(resolve_path(root, &path).is_empty()).to(be_true());
  }
}
