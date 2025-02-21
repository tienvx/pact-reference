//! Types for supporting building and executing plans for bodies

use std::fmt::Debug;
use std::sync::{Arc, LazyLock, RwLock};

use bytes::Bytes;
use nom::AsBytes;
use serde_json::Value;

use pact_models::content_types::ContentType;
use pact_models::path_exp::DocPath;

use crate::engine::{ExecutionPlanNode, NodeValue, PlanMatchingContext};

/// Trait for implementations of builders for different types of bodies
pub trait PlanBodyBuilder: Debug {
  /// If this builder supports a namespace for nodes.
  fn namespace(&self) -> Option<String> {
    None
  }

  /// If this builder supports the given content type
  fn supports_type(&self, content_type: &ContentType) -> bool;

  /// Build the plan for the expected body
  fn build_plan(&self, content: &Bytes, context: &PlanMatchingContext) -> anyhow::Result<ExecutionPlanNode>;
}

static BODY_PLAN_BUILDERS: LazyLock<RwLock<Vec<Arc<dyn PlanBodyBuilder + Send + Sync>>>> = LazyLock::new(|| {
  let mut builders: Vec<Arc<dyn PlanBodyBuilder + Send + Sync>> = vec![];

  // TODO: Add default implementations here
  builders.push(Arc::new(JsonPlanBuilder::new()));

  RwLock::new(builders)
});

pub(crate) fn get_body_plan_builder(content_type: &ContentType) -> Option<Arc<dyn PlanBodyBuilder + Send + Sync>> {
  let registered_builders = (*BODY_PLAN_BUILDERS).read().unwrap();
  registered_builders.iter().find(|builder| builder.supports_type(content_type))
    .cloned()
}

/// Plan builder for plain text. This just sets up an equality matcher
#[derive(Clone, Debug)]
pub struct PlainTextBuilder;

impl PlainTextBuilder {
  /// Create a new instance
  pub fn new() -> Self {
    PlainTextBuilder{}
  }
}

impl PlanBodyBuilder for PlainTextBuilder {
  fn supports_type(&self, content_type: &ContentType) -> bool {
    content_type.is_text()
  }

  fn build_plan(&self, content: &Bytes, _context: &PlanMatchingContext) -> anyhow::Result<ExecutionPlanNode> {
    let bytes = content.to_vec();
    let text_content = String::from_utf8_lossy(&bytes);
    let mut node = ExecutionPlanNode::action("match:equality");
    let mut child_node = ExecutionPlanNode::action("convert:UTF8");
    child_node.add(ExecutionPlanNode::resolve_value(DocPath::new_unwrap("$.body")));
    node.add(child_node);
    node.add(ExecutionPlanNode::value_node(text_content.to_string()));
    Ok(node)
  }
}

/// Plan builder for JSON bodies
#[derive(Clone, Debug)]
pub struct JsonPlanBuilder;

impl JsonPlanBuilder {
  /// Create a new instance
  pub fn new() -> Self {
    JsonPlanBuilder{}
  }
}

impl PlanBodyBuilder for JsonPlanBuilder {
  fn namespace(&self) -> Option<String> {
    Some("json".to_string())
  }

  fn supports_type(&self, content_type: &ContentType) -> bool {
    content_type.is_json()
  }

  fn build_plan(&self, content: &Bytes, context: &PlanMatchingContext) -> anyhow::Result<ExecutionPlanNode> {
    let expected_json: Value = serde_json::from_slice(content.as_bytes())?;
    let path = DocPath::root();
    let mut apply_node = ExecutionPlanNode::apply();
    apply_node
      .add(ExecutionPlanNode::action("json:parse")
        .add(ExecutionPlanNode::resolve_value(DocPath::new_unwrap("$.body"))));

    match &expected_json {
      Value::Array(items) => {
        // TODO: Deal with matching rules here
        if context.matcher_is_defined(&path) {
          todo!("Deal with matching rules here")
        } else if items.is_empty() {
          apply_node.add(
            ExecutionPlanNode::action("json:expect:empty")
              .add(ExecutionPlanNode::value_node("ARRAY"))
              .add(ExecutionPlanNode::action("apply"))
          );
        } else {
          apply_node.add(ExecutionPlanNode::action("push"));
          apply_node.add(
            ExecutionPlanNode::action("json:match:length")
              .add(ExecutionPlanNode::value_node("ARRAY"))
              .add(ExecutionPlanNode::value_node(items.len()))
              .add(ExecutionPlanNode::action("apply"))
          );
          apply_node.add(ExecutionPlanNode::action("pop"));
          let mut iter_node = ExecutionPlanNode::container("$");

          for (index, item) in items.iter().enumerate() {
            let item_path = path.join_index(index);
            let mut item_node = ExecutionPlanNode::container(item_path.clone());
            match item {
              Value::Array(_) => { todo!() }
              Value::Object(_) => { todo!() }
              _ => {
                let mut presence_check = ExecutionPlanNode::action("if");
                presence_check
                  .add(
                    ExecutionPlanNode::action("check:exists")
                      .add(ExecutionPlanNode::resolve_current_value(item_path.clone()))
                  )
                  .add(
                    ExecutionPlanNode::action("match:equality")
                      .add(ExecutionPlanNode::resolve_current_value(item_path))
                      .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), item.to_string())))
                  );
                item_node.add(presence_check);
              }
            }
            iter_node.add(item_node);
          }

          apply_node.add(iter_node);
        }
      }
      Value::Object(entries) => {
        // TODO: Deal with matching rules here
        if context.matcher_is_defined(&path) {
          todo!("Deal with matching rules here")
        } else if entries.is_empty() {
          apply_node.add(
            ExecutionPlanNode::action("json:expect:empty")
              .add(ExecutionPlanNode::value_node("OBJECT"))
              .add(ExecutionPlanNode::action("apply"))
          );
        } else {
          apply_node.add(ExecutionPlanNode::action("push"));
          if !context.allow_unexpected_entries {
            apply_node.add(
              ExecutionPlanNode::action("json:expect:entries")
                .add(ExecutionPlanNode::value_node("OBJECT"))
                .add(ExecutionPlanNode::value_node(NodeValue::SLIST(
                  entries.keys().map(|key| key.clone()).collect())
                ))
                .add(ExecutionPlanNode::action("apply"))
            );
          } else {
            apply_node.add(
              ExecutionPlanNode::action("json:expect:not-empty")
                .add(ExecutionPlanNode::value_node("OBJECT"))
                .add(ExecutionPlanNode::action("apply"))
            );
          }
          apply_node.add(ExecutionPlanNode::action("pop"));
          let mut iter_node = ExecutionPlanNode::container("$");

          for (key, value) in entries {
            let item_path = path.join(key);
            let mut item_node = ExecutionPlanNode::container(item_path.clone());
            match value {
              Value::Array(_) => { todo!() }
              Value::Object(_) => { todo!() }
              _ => {
                item_node.add(
                  ExecutionPlanNode::action("match:equality")
                    .add(ExecutionPlanNode::resolve_current_value(item_path))
                    .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), value.to_string())))
                );
              }
            }
            iter_node.add(item_node);
          }

          apply_node.add(iter_node);
        }
      }
      _ => {
        apply_node.add(
          ExecutionPlanNode::action("match:equality")
            .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), expected_json.to_string())))
            .add(ExecutionPlanNode::action("apply"))
        );
      }
    }

    Ok(apply_node)
  }
}

#[cfg(test)]
mod tests {
  use bytes::Bytes;
  use pretty_assertions::assert_eq;
  use serde_json::{json, Value};

  use crate::engine::bodies::{JsonPlanBuilder, PlanBodyBuilder};
  use crate::engine::context::PlanMatchingContext;

  #[test]
  fn json_plan_builder_with_null() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(Value::Null.to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:null,
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_boolean() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(Value::Bool(true).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:true,
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_string() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(Value::String("I am a string!".to_string()).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:"I am a string!",
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_int() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!(1000).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:1000,
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_float() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!(1000.3).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:1000.3,
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_empty_array() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!([]).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %json:expect:empty (
    'ARRAY',
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_array() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!([100, 200, 300]).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %push (),
  %json:match:length (
    'ARRAY',
    UINT(3),
    %apply ()
  ),
  %pop (),
  :$ (
    :$[0] (
      %if (
        %check:exists (
          ~>$[0]
        ),
        %match:equality (
          ~>$[0],
          json:100
        )
      )
    ),
    :$[1] (
      %if (
        %check:exists (
          ~>$[1]
        ),
        %match:equality (
          ~>$[1],
          json:200
        )
      )
    ),
    :$[2] (
      %if (
        %check:exists (
          ~>$[2]
        ),
        %match:equality (
          ~>$[2],
          json:300
        )
      )
    )
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_empty_object() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!({}).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %json:expect:empty (
    'OBJECT',
    %apply ()
  )
)"#);
  }

  #[test]
  fn json_plan_builder_with_object() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!({"a": 100, "b": 200, "c": 300})
      .to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(buffer,
r#"-> (
  %json:parse (
    $.body
  ),
  %push (),
  %json:expect:entries (
    'OBJECT',
    ['a', 'b', 'c'],
    %apply ()
  ),
  %pop (),
  :$ (
    :$.a (
      %match:equality (
        ~>$.a,
        json:100
      )
    ),
    :$.b (
      %match:equality (
        ~>$.b,
        json:200
      )
    ),
    :$.c (
      %match:equality (
        ~>$.c,
        json:300
      )
    )
  )
)"#);
  }
}
