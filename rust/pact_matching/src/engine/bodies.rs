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
    node.add(ExecutionPlanNode::value_node(text_content.to_string()));
    node.add(child_node);
    node.add(ExecutionPlanNode::value_node(NodeValue::NULL));
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
    let mut apply_node = ExecutionPlanNode::action("tee");
    apply_node
      .add(ExecutionPlanNode::action("json:parse")
        .add(ExecutionPlanNode::resolve_value(DocPath::new_unwrap("$.body"))));

    match &expected_json {
      Value::Array(items) => {
        let mut root_node = ExecutionPlanNode::container("$");
        // TODO: Deal with matching rules here
        if context.matcher_is_defined(&path) {
          todo!("Deal with matching rules here")
        } else if items.is_empty() {
          root_node.add(
            ExecutionPlanNode::action("json:expect:empty")
              .add(ExecutionPlanNode::value_node("ARRAY"))
              .add(ExecutionPlanNode::action("apply"))
          );
        } else {
          root_node.add(
            ExecutionPlanNode::action("json:match:length")
              .add(ExecutionPlanNode::value_node("ARRAY"))
              .add(ExecutionPlanNode::value_node(items.len()))
              .add(ExecutionPlanNode::action("apply"))
          );

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
                      .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), item.to_string())))
                      .add(ExecutionPlanNode::resolve_current_value(item_path))
                      .add(ExecutionPlanNode::value_node(NodeValue::NULL))
                  );
                item_node.add(presence_check);
              }
            }
            root_node.add(item_node);
          }
        }
        apply_node.add(root_node);
      }
      Value::Object(entries) => {
        let mut root_node = ExecutionPlanNode::container("$");
        // TODO: Deal with matching rules here
        if context.matcher_is_defined(&path) {
          todo!("Deal with matching rules here")
        } else if entries.is_empty() {
          root_node.add(
            ExecutionPlanNode::action("json:expect:empty")
              .add(ExecutionPlanNode::value_node("OBJECT"))
              .add(ExecutionPlanNode::action("apply"))
          );
        } else {
          if !context.config.allow_unexpected_entries {
            root_node.add(
              ExecutionPlanNode::action("json:expect:entries")
                .add(ExecutionPlanNode::value_node("OBJECT"))
                .add(ExecutionPlanNode::value_node(NodeValue::SLIST(
                  entries.keys().map(|key| key.clone()).collect())
                ))
                .add(ExecutionPlanNode::action("apply"))
            );
          } else {
            root_node.add(
              ExecutionPlanNode::action("json:expect:not-empty")
                .add(ExecutionPlanNode::value_node("OBJECT"))
                .add(ExecutionPlanNode::action("apply"))
            );
          }

          for (key, value) in entries {
            let item_path = path.join(key);
            let mut item_node = ExecutionPlanNode::container(item_path.clone());
            match value {
              Value::Array(_) => { todo!() }
              Value::Object(_) => { todo!() }
              _ => {
                item_node.add(
                  ExecutionPlanNode::action("match:equality")
                    .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), value.to_string())))
                    .add(ExecutionPlanNode::resolve_current_value(item_path))
                    .add(ExecutionPlanNode::value_node(NodeValue::NULL))
                );
              }
            }
            root_node.add(item_node);
          }
        }
        apply_node.add(root_node);
      }
      _ => {
        apply_node.add(
          ExecutionPlanNode::action("match:equality")
            .add(ExecutionPlanNode::value_node(NodeValue::NAMESPACED("json".to_string(), expected_json.to_string())))
            .add(ExecutionPlanNode::action("apply"))
            .add(ExecutionPlanNode::value_node(NodeValue::NULL))
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
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:null,
    %apply (),
    NULL
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_boolean() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(Value::Bool(true).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:true,
    %apply (),
    NULL
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_string() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(Value::String("I am a string!".to_string()).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:"I am a string!",
    %apply (),
    NULL
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_int() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!(1000).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:1000,
    %apply (),
    NULL
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_float() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!(1000.3).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  %match:equality (
    json:1000.3,
    %apply (),
    NULL
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_empty_array() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!([]).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  :$ (
    %json:expect:empty (
      'ARRAY',
      %apply ()
    )
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_array() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!([100, 200, 300]).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  :$ (
    %json:match:length (
      'ARRAY',
      UINT(3),
      %apply ()
    ),
    :$[0] (
      %if (
        %check:exists (
          ~>$[0]
        ),
        %match:equality (
          json:100,
          ~>$[0],
          NULL
        )
      )
    ),
    :$[1] (
      %if (
        %check:exists (
          ~>$[1]
        ),
        %match:equality (
          json:200,
          ~>$[1],
          NULL
        )
      )
    ),
    :$[2] (
      %if (
        %check:exists (
          ~>$[2]
        ),
        %match:equality (
          json:300,
          ~>$[2],
          NULL
        )
      )
    )
  )
)"#, buffer);
  }

  #[test]
  fn json_plan_builder_with_empty_object() {
    let builder = JsonPlanBuilder::new();
    let context = PlanMatchingContext::default();
    let content = Bytes::copy_from_slice(json!({}).to_string().as_bytes());
    let node = builder.build_plan(&content, &context).unwrap();
    let mut buffer = String::new();
    node.pretty_form(&mut buffer, 0);
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  :$ (
    %json:expect:empty (
      'OBJECT',
      %apply ()
    )
  )
)"#, buffer);
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
    assert_eq!(r#"%tee (
  %json:parse (
    $.body
  ),
  :$ (
    %json:expect:entries (
      'OBJECT',
      ['a', 'b', 'c'],
      %apply ()
    ),
    :$.a (
      %match:equality (
        json:100,
        ~>$.a,
        NULL
      )
    ),
    :$.b (
      %match:equality (
        json:200,
        ~>$.b,
        NULL
      )
    ),
    :$.c (
      %match:equality (
        json:300,
        ~>$.c,
        NULL
      )
    )
  )
)"#, buffer);
  }
}
