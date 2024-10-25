//! Structs and traits to support a general matching engine

use std::panic::RefUnwindSafe;

use pact_models::bodies::OptionalBody;
use pact_models::content_types::{ContentType, TEXT};
use pact_models::path_exp::DocPath;
use pact_models::v4::http_parts::HttpRequest;
use pact_models::v4::interaction::V4Interaction;
use pact_models::v4::pact::V4Pact;
use pact_models::v4::synch_http::SynchronousHttp;

/// Enum for the type of Plan Node
#[derive(Clone, Debug, Default)]
pub enum PlanNodeType {
  /// Default plan node is empty
  #[default]
  EMPTY,
  /// Container node with a label
  CONTAINER(String),
  /// Action node with a function reference
  ACTION(String),
  /// Leaf node that contains a value
  VALUE(NodeValue),
  /// Leaf node that stores an expression to resolve against the test context
  RESOLVE(DocPath),
}

/// Enum for the value stored in a leaf node
#[derive(Clone, Debug, Default)]
pub enum NodeValue {
  /// Default is no value
  #[default]
  NULL,
  /// A string value
  STRING(String),
}

impl NodeValue {
  /// Returns the encoded string form of the node value
  pub fn str_form(&self) -> String {
    match self {
      NodeValue::NULL => "NULL".to_string(),
      NodeValue::STRING(str) => format!("\"{}\"", str)
    }
  }
}

impl From<String> for NodeValue {
  fn from(value: String) -> Self {
    NodeValue::STRING(value.clone())
  }
}

impl From<&str> for NodeValue {
  fn from(value: &str) -> Self {
    NodeValue::STRING(value.to_string())
  }
}

/// Enum to store the result of executing a node
#[derive(Clone, Debug, Default)]
pub enum NodeResult {
  /// Default value to make a node as successfully executed
  #[default]
  OK,
  /// Marks a node as successfully executed with a result
  VALUE(NodeValue),
  /// Marks a node as unsuccessfully executed with an error
  ERROR(String)
}

/// Node in an executable plan tree
#[derive(Clone, Debug, Default)]
pub struct ExecutionPlanNode {
  /// Type of the node
  pub node_type: PlanNodeType,
  /// Any result associated with the node
  pub result: Option<NodeResult>,
  /// Child nodes
  pub children: Vec<ExecutionPlanNode>
}

impl ExecutionPlanNode {
  /// Returns the human-readable text from of the node
  pub fn pretty_form(&self, buffer: &mut String, indent: usize) {
    let pad = " ".repeat(indent);

    match &self.node_type {
      PlanNodeType::EMPTY => {}
      PlanNodeType::CONTAINER(label) => {
        buffer.push_str(pad.as_str());
        buffer.push(':');
        if label.contains(|ch: char| ch.is_whitespace()) {
          buffer.push_str(format!("\"{}\"", label).as_str());
        } else {
          buffer.push_str(label.as_str());
        }
        if self.is_empty() {
          buffer.push_str(" ()");
        } else {
          buffer.push_str(" (\n");
          self.pretty_form_children(buffer, indent);
          buffer.push_str(pad.as_str());
          buffer.push(')');
        }
      }
      PlanNodeType::ACTION(value) => {
        buffer.push_str(pad.as_str());
        buffer.push('%');
        buffer.push_str(value.as_str());
        if self.is_empty() {
          buffer.push_str(" ()");
        } else {
          buffer.push_str(" (\n");
          self.pretty_form_children(buffer, indent);
          buffer.push_str(pad.as_str());
          buffer.push(')');
        }
      }
      PlanNodeType::VALUE(value) => {
        buffer.push_str(pad.as_str());
        buffer.push_str(value.str_form().as_str());
      }
      PlanNodeType::RESOLVE(str) => {
        buffer.push_str(pad.as_str());
        buffer.push_str(str.to_string().as_str());
      }
    }
  }

  fn pretty_form_children(&self, buffer: &mut String, indent: usize) {
    let len = self.children.len();
    for (index, child) in self.children.iter().enumerate() {
      child.pretty_form(buffer, indent + 2);
      if index < len - 1 {
        buffer.push(',');
      }
      buffer.push('\n');
    }
  }

  /// Returns the serialised text form of the node
  pub fn str_form(&self) -> String {
    let mut buffer = String::new();
    buffer.push('(');

    match &self.node_type {
      PlanNodeType::EMPTY => {}
      PlanNodeType::CONTAINER(label) => {
        buffer.push(':');
        if label.contains(|ch: char| ch.is_whitespace()) {
          buffer.push_str(format!("\"{}\"", label).as_str());
        } else {
          buffer.push_str(label.as_str());
        }
        buffer.push('(');
        self.str_form_children(&mut buffer);
        buffer.push(')');
      }
      PlanNodeType::ACTION(value) => {
        buffer.push('%');
        buffer.push_str(value.as_str());
        buffer.push('(');
        self.str_form_children(&mut buffer);
        buffer.push(')');
      }
      PlanNodeType::VALUE(value) => {
        buffer.push_str(value.str_form().as_str());
      }
      PlanNodeType::RESOLVE(str) => {
        buffer.push_str(str.to_string().as_str());
      }
    }

    buffer.push(')');
    buffer
  }

  fn str_form_children(&self, buffer: &mut String) {
    let len = self.children.len();
    for (index, child) in self.children.iter().enumerate() {
      buffer.push_str(child.str_form().as_str());
      if index < len - 1 {
        buffer.push(',');
      }
    }
  }

  /// Constructor for a container node
  pub fn container(label: &str) -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::CONTAINER(label.to_string()),
      result: None,
      children: vec![],
    }
  }

  /// Constructor for an action node
  pub fn action(value: &str) -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::ACTION(value.to_string()),
      result: None,
      children: vec![],
    }
  }

  /// Constructor for a value node
  pub fn value<T: Into<NodeValue>>(value: T) -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::VALUE(value.into()),
      result: None,
      children: vec![],
    }
  }

  /// Constructor for a resolve node
  pub fn resolve_value<T: Into<DocPath>>(resolve_str: T) -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::RESOLVE(resolve_str.into()),
      result: None,
      children: vec![],
    }
  }

  /// Adds the node as a child
  pub fn add<N>(&mut self, node: N) -> &mut Self where N: Into<ExecutionPlanNode> {
    self.children.push(node.into());
    self
  }

  /// If the node is a leaf node
  pub fn is_empty(&self) -> bool {
    match self.node_type {
      PlanNodeType::EMPTY => true,
      _ => self.children.is_empty()
    }
  }
}

impl From<&mut ExecutionPlanNode> for ExecutionPlanNode {
  fn from(value: &mut ExecutionPlanNode) -> Self {
    value.clone()
  }
}

/// An executable plan that contains a tree of execution nodes
#[derive(Clone, Debug, Default)]
pub struct ExecutionPlan {
  pub plan_root: ExecutionPlanNode
}

impl ExecutionPlan {
  /// Creates a new empty execution plan with a single root container
  fn new(label: &str) -> ExecutionPlan {
    ExecutionPlan {
      plan_root: ExecutionPlanNode::container(label)
    }
  }

  /// Adds the node as the root node if the node is not empty (i.e. not a leaf node).
  pub fn add(&mut self, node: ExecutionPlanNode) {
    if !node.is_empty() {
      self.plan_root.add(node);
    }
  }

  /// Returns the serialised text form of the execution  plan.
  pub fn str_form(&self) -> String {
    let mut buffer = String::new();
    buffer.push('(');
    buffer.push_str(self.plan_root.str_form().as_str());
    buffer.push(')');
    buffer
  }

  /// Returns the human-readable text form of the execution plan.
  pub fn pretty_form(&self) -> String {
    let mut buffer = String::new();
    buffer.push_str("(\n");
    self.plan_root.pretty_form(&mut buffer, 2);
    buffer.push_str("\n)\n");
    buffer
  }
}

/// Context to store data for use in executing an execution plan.
#[derive(Clone, Debug)]
pub struct PlanMatchingContext {
  /// Pact the plan is for
  pub pact: V4Pact,
  /// Interaction that the plan id for
  pub interaction: Box<dyn V4Interaction + Send + Sync + RefUnwindSafe>
}

impl Default for PlanMatchingContext {
  fn default() -> Self {
    PlanMatchingContext {
      pact: Default::default(),
      interaction: Box::new(SynchronousHttp::default())
    }
  }
}

/// Constructs an execution plan for the HTTP request part.
pub fn build_request_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlan> {
  let mut plan = ExecutionPlan::new("request");

  plan.add(setup_method_plan(expected, context)?);
  plan.add(setup_path_plan(expected, context)?);
  plan.add(setup_query_plan(expected, context)?);
  plan.add(setup_header_plan(expected, context)?);
  plan.add(setup_body_plan(expected, context)?);

  Ok(plan)
}

fn setup_method_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  let mut method_container = ExecutionPlanNode::container("method");

  let mut match_method = ExecutionPlanNode::action("match:equality");
  match_method
    .add(ExecutionPlanNode::action("upper-case")
      .add(ExecutionPlanNode::resolve_value(DocPath::new("$.method")?)))
    .add(ExecutionPlanNode::value(expected.method.as_str()));

  // TODO: Look at the matching rules and generators here
  method_container.add(match_method);

  Ok(method_container)
}

fn setup_path_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  // TODO: Look at the matching rules and generators here
  let mut plan_node = ExecutionPlanNode::container("path");
  plan_node
    .add(
      ExecutionPlanNode::action("match:equality")
        .add(ExecutionPlanNode::resolve_value(DocPath::new("$.path")?))
        .add(ExecutionPlanNode::value(expected.path.as_str()))
    );
  Ok(plan_node)
}

fn setup_query_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  // TODO: Look at the matching rules and generators here
  let mut plan_node = ExecutionPlanNode::container("query parameters");

  if let Some(query) = &expected.query {
    if query.is_empty() {
      plan_node
        .add(
          ExecutionPlanNode::action("expect:empty")
            .add(ExecutionPlanNode::resolve_value(DocPath::new("$.query")?))
        );
    } else {
      todo!()
    }
  } else {
    plan_node
      .add(
        ExecutionPlanNode::action("expect:empty")
          .add(ExecutionPlanNode::resolve_value(DocPath::new("$.query")?))
      );
  }

  Ok(plan_node)
}

fn setup_header_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  // TODO: Look at the matching rules and generators here
  let mut plan_node = ExecutionPlanNode::container("headers");

  if let Some(headers) = &expected.headers {
    if !headers.is_empty() {
      todo!()
    }
  }

  Ok(plan_node)
}

fn setup_body_plan(
  expected: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  // TODO: Look at the matching rules and generators here
  let mut plan_node = ExecutionPlanNode::container("body");

  match &expected.body {
    OptionalBody::Missing => {
      todo!()
    }
    OptionalBody::Empty => {
      todo!()
    }
    OptionalBody::Null => {
      todo!()
    }
    OptionalBody::Present(content, _, _) => {
      let content_type = expected.content_type().unwrap_or_else(|| TEXT.clone());
      let mut content_type_check_node = ExecutionPlanNode::action("if");
      content_type_check_node
        .add(
          ExecutionPlanNode::action("match:equality")
            .add(ExecutionPlanNode::action("content-type"))
            .add(ExecutionPlanNode::value(content_type.to_string()))
        );
      if content_type.is_json() {

      } else {
        todo!()
      }
      plan_node.add(content_type_check_node);
    }
  }

  Ok(plan_node)
}

pub fn execute_request_plan(
  plan: &ExecutionPlan,
  actual: &HttpRequest,
  context: &PlanMatchingContext
) -> anyhow::Result<ExecutionPlan> {
  Ok(ExecutionPlan::default())
}

#[cfg(test)]
mod tests {
  use expectest::prelude::*;
  use serde_json::json;
  use pact_models::bodies::OptionalBody;
  use pact_models::v4::http_parts::HttpRequest;
  use pretty_assertions::assert_eq;

  use crate::engine::{build_request_plan, execute_request_plan, ExecutionPlan, PlanMatchingContext};

  #[test]
  fn simple_match_request_test() -> anyhow::Result<()> {
    let request = HttpRequest {
      method: "POST".to_string(),
      path: "/test".to_string(),
      query: None,
      headers: None,
      body: OptionalBody::from(&json!({
        "b": "22"
      })),
      matching_rules: Default::default(),
      generators: Default::default(),
    };
    let expected_request = HttpRequest {
      method: "POST".to_string(),
      path: "/test".to_string(),
      query: None,
      headers: None,
      body: OptionalBody::from(&json!({
        "a": 100,
        "b": 200.1
      })),
      matching_rules: Default::default(),
      generators: Default::default(),
    };
    let context = PlanMatchingContext::default();
    let plan = build_request_plan(&expected_request, &context)?;

    assert_eq!(plan.pretty_form(),
r#"(
  :request (
    :method (
      %match:equality (
        %upper-case (
          $.method
        ),
        "POST"
      )
    ),
    :path (
      %match:equality (
        $.path,
        "/test"
      )
    ),
    :"query parameters" (
      %expect:empty (
        $.query
      )
    ),
    :body (
      %if (
        %match:equality (
          %content-type (),
          "application/json;charset=utf-8"
        ),
        :body:$ (
          :body:$:a (
            %if (
              %expect:present ($.body."$.a"),
              %match:equality ($.body."$.a", 100)
            )
          ),
          :body:$:b (
            %if (
              %expect:present ($.body."$.b"),
              %match:equality ($.body."$.b", 200.1)
            )
          )
        )
      )
    )
  )
)
"#);

    let executed_plan = execute_request_plan(&plan, &request, &context)?;
    assert_eq!(executed_plan.pretty_form(), r#"(
      :request (
        :method (
          %match:equality (
            %upper-case (
              $.method ~ "POST"
            ),
            "POST" ~ OK
          )
        ),
        :path (
          %match:equality ($.path ~ "/test", "/test") ~ OK
        ),
        :"query parameters" (
          %expect:empty ($.query ~ {}) ~ OK
        ),
        :body (
          %if (
            %match:equality (%content-type () ~ "application/json", "application/json;charset=utf-8") ~ OK,
            :body:$ (
              :body:$:a (
                %if (
                  %expect:present ($.body."$.a" ~ NULL) ~ ERROR(Expected attribute "$.a" but it was missing),
                  %match:equality ($.body."$.a", 100) ~ NULL
                )
              ),
              :body:$:b (
                %if (
                  %expect:present ($.body."$.b" ~ "22") ~ OK,
                  %match:equality ($.body."$.b" ~ "22", 200.1) ~ ERROR(Expected attribute "$.b" to equal "22" (String) but it was 200.1 (Double))
                )
              )
            )
          )
        )
      )
    )
    "#);

    Ok(())
  }
}
