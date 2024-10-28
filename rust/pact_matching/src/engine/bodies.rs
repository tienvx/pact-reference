//! Types for supporting building and executing plans for bodies

use std::fmt::Debug;
use std::sync::{Arc, LazyLock, RwLock};

use bytes::Bytes;

use pact_models::content_types::ContentType;
use pact_models::path_exp::DocPath;

use crate::engine::{ExecutionPlanNode, PlanMatchingContext};

/// Trait for implementations of builders for different types of bodies
pub trait PlanBodyBuilder: Debug {
  /// If this builder supports the given content type
  fn supports_type(&self, content_type: &ContentType) -> bool;

  /// Build the plan for the expected body
  fn build_plan(&self, content: &Bytes, context: &PlanMatchingContext) -> anyhow::Result<ExecutionPlanNode>;
}

static BODY_PLAN_BUILDERS: LazyLock<RwLock<Vec<Arc<dyn PlanBodyBuilder + Send + Sync>>>> = LazyLock::new(|| {
  let mut builders = vec![];
  // TODO: Add default implementations here
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
