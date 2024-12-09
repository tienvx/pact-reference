//! Structs and traits to support a general matching engine

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::panic::RefUnwindSafe;

use anyhow::anyhow;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use itertools::Itertools;
use onig::EncodedChars;
use snailquote::escape;
use tracing::{instrument, Level, trace};

use pact_models::bodies::OptionalBody;
use pact_models::content_types::TEXT;
use pact_models::matchingrules::MatchingRule;
use pact_models::path_exp::DocPath;
use pact_models::v4::http_parts::HttpRequest;
use pact_models::v4::interaction::V4Interaction;
use pact_models::v4::pact::V4Pact;
use pact_models::v4::synch_http::SynchronousHttp;

use crate::engine::bodies::{get_body_plan_builder, PlainTextBuilder, PlanBodyBuilder};
use crate::engine::context::PlanMatchingContext;
use crate::engine::value_resolvers::{HttpRequestValueResolver, ValueResolver};
use crate::matchers::Matches;

mod bodies;
mod value_resolvers;
mod context;

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
  /// Pipeline node (apply), which applies each node to the next as a pipeline returning the last
  PIPELINE
}

/// Enum for the value stored in a leaf node
#[derive(Clone, Debug, Default, PartialEq)]
pub enum NodeValue {
  /// Default is no value
  #[default]
  NULL,
  /// A string value
  STRING(String),
  /// Boolean value
  BOOL(bool),
  /// Multi-string map (String key to one or more string values)
  MMAP(HashMap<String, Vec<String>>),
  /// List of String values
  SLIST(Vec<String>),
  /// Byte Array
  BARRAY(Vec<u8>),
  /// Namespaced value
  NAMESPACED(String, String)
}

impl NodeValue {
  /// Returns the encoded string form of the node value
  pub fn str_form(&self) -> String {
    match self {
      NodeValue::NULL => "NULL".to_string(),
      NodeValue::STRING(str) => {
        Self::escape_string(str)
      }
      NodeValue::BOOL(b) => {
        format!("BOOL({})", b)
      }
      NodeValue::MMAP(map) => {
        let mut buffer = String::new();
        buffer.push('{');

        let mut first = true;
        for (key, values) in map {
          if first {
            first = false;
          } else {
            buffer.push_str(", ");
          }
          buffer.push_str(Self::escape_string(key).as_str());
          if values.is_empty() {
            buffer.push_str(": []");
          } else if values.len() == 1 {
            buffer.push_str(": ");
            buffer.push_str(Self::escape_string(&values[0]).as_str());
          } else {
            buffer.push_str(": [");
            buffer.push_str(values.iter().map(|v| Self::escape_string(v)).join(", ").as_str());
            buffer.push(']');
          }
        }

        buffer.push('}');
        buffer
      }
      NodeValue::SLIST(list) => {
        let mut buffer = String::new();
        buffer.push('[');
        buffer.push_str(list.iter().map(|v| Self::escape_string(v)).join(", ").as_str());
        buffer.push(']');
        buffer
      }
      NodeValue::BARRAY(bytes) => {
        let mut buffer = String::new();
        buffer.push_str("BYTES(");
        buffer.push_str(bytes.len().to_string().as_str());
        buffer.push_str(", ");
        buffer.push_str(BASE64.encode(bytes).as_str());
        buffer.push(')');
        buffer
      }
      NodeValue::NAMESPACED(name, value) => {
        let mut buffer = String::new();
        buffer.push_str(name);
        buffer.push(':');
        buffer.push_str(value);
        buffer
      }
    }
  }

  fn escape_string(str: &String) -> String {
    let escaped_str = escape(str);
    if let Cow::Borrowed(_) = &escaped_str {
      format!("'{}'", escaped_str)
    } else {
      escaped_str.to_string()
    }
  }

  /// Returns the type of the value
  pub fn value_type(&self) -> &str {
    match self {
      NodeValue::NULL => "NULL",
      NodeValue::STRING(_) => "String",
      NodeValue::BOOL(_) => "Boolean",
      NodeValue::MMAP(_) => "Multi-Value String Map",
      NodeValue::SLIST(_) => "String List",
      NodeValue::BARRAY(_) => "Byte Array",
      NodeValue::NAMESPACED(_, _) => "Namespaced Value"
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

impl Matches<NodeValue> for NodeValue {
  fn matches_with(&self, actual: NodeValue, matcher: &MatchingRule, cascaded: bool) -> anyhow::Result<()> {
    match matcher {
      MatchingRule::Equality => if self == &actual {
        Ok(())
      } else {
        Err(anyhow!("Expected {} to equal {}", self.str_form(), actual.str_form()))
      }
      MatchingRule::Regex(_) => todo!(),
      MatchingRule::Type => todo!(),
      MatchingRule::MinType(_) => todo!(),
      MatchingRule::MaxType(_) => todo!(),
      MatchingRule::MinMaxType(_, _) => todo!(),
      MatchingRule::Timestamp(_) => todo!(),
      MatchingRule::Time(_) => todo!(),
      MatchingRule::Date(_) => todo!(),
      MatchingRule::Include(_) => todo!(),
      MatchingRule::Number => todo!(),
      MatchingRule::Integer => todo!(),
      MatchingRule::Decimal => todo!(),
      MatchingRule::Null => todo!(),
      MatchingRule::ContentType(_) => todo!(),
      MatchingRule::ArrayContains(_) => todo!(),
      MatchingRule::Values => todo!(),
      MatchingRule::Boolean => todo!(),
      MatchingRule::StatusCode(_) => todo!(),
      MatchingRule::NotEmpty => todo!(),
      MatchingRule::Semver => todo!(),
      MatchingRule::EachKey(_) => todo!(),
      MatchingRule::EachValue(_) => todo!()
    }
  }
}

/// Enum to store the result of executing a node
#[derive(Clone, Debug, Default, PartialEq)]
pub enum NodeResult {
  /// Default value to make a node as successfully executed
  #[default]
  OK,
  /// Marks a node as successfully executed with a result
  VALUE(NodeValue),
  /// Marks a node as unsuccessfully executed with an error
  ERROR(String)
}

impl NodeResult {
  /// Return the OR of this result with the given one
  pub fn or(&self, option: &Option<NodeResult>) -> NodeResult {
    if let Some(result) = option {
      match result {
        NodeResult::OK => match self {
          NodeResult::OK => NodeResult::OK,
          NodeResult::VALUE(_) => NodeResult::OK,
          NodeResult::ERROR(_) => NodeResult::ERROR("One or more children failed".to_string())
        },
        NodeResult::VALUE(_) => match self {
          NodeResult::OK => NodeResult::OK,
          NodeResult::VALUE(_) => NodeResult::OK,
          NodeResult::ERROR(_) => NodeResult::ERROR("One or more children failed".to_string())
        }
        NodeResult::ERROR(_) => NodeResult::ERROR("One or more children failed".to_string())
      }
    } else {
      self.clone()
    }
  }

  /// Converts the result value to a string
  pub fn as_string(&self) -> Option<String> {
    match self {
      NodeResult::OK => None,
      NodeResult::VALUE(val) => match val {
        NodeValue::NULL => Some("".to_string()),
        NodeValue::STRING(s) => Some(s.clone()),
        NodeValue::BOOL(b) => Some(b.to_string()),
        NodeValue::MMAP(m) => Some(format!("{:?}", m)),
        NodeValue::SLIST(list) => Some(format!("{:?}", list)),
        NodeValue::BARRAY(bytes) => Some(BASE64.encode(bytes)),
        NodeValue::NAMESPACED(name, value) => Some(format!("{}:{}", name, value))
      }
      NodeResult::ERROR(_) => None
    }
  }

  /// Returns the associated value if there is one
  pub fn as_value(&self) -> Option<NodeValue> {
    match self {
      NodeResult::OK => None,
      NodeResult::VALUE(val) => Some(val.clone()),
      NodeResult::ERROR(_) => None
    }
  }

  /// If this value represents a truthy value (not NULL, false ot empty)
  pub fn is_truthy(&self) -> bool {
    match self {
      NodeResult::OK => true,
      NodeResult::VALUE(v) => match v {
        NodeValue::NULL => false,
        NodeValue::STRING(s) => !s.is_empty(),
        NodeValue::BOOL(b) => *b,
        NodeValue::MMAP(m) => !m.is_empty(),
        NodeValue::SLIST(l) => !l.is_empty(),
        NodeValue::BARRAY(b) => !b.is_empty(),
        NodeValue::NAMESPACED(_, _) => false // TODO: Need a way to resolve this
      }
      NodeResult::ERROR(_) => false
    }
  }
}

impl Display for NodeResult {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      NodeResult::OK => write!(f, "OK"),
      NodeResult::VALUE(val) => write!(f, "{}", val.str_form()),
      NodeResult::ERROR(err) => write!(f, "ERROR({})", err),
    }
  }
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

        if let Some(result) = &self.result {
          buffer.push_str(" ~ ");
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::VALUE(value) => {
        buffer.push_str(pad.as_str());
        buffer.push_str(value.str_form().as_str());

        if let Some(result) = &self.result {
          buffer.push_str(" ~ ");
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::RESOLVE(str) => {
        buffer.push_str(pad.as_str());
        buffer.push_str(str.to_string().as_str());

        if let Some(result) = &self.result {
          buffer.push_str(" ~ ");
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::PIPELINE => {
        buffer.push_str(pad.as_str());
        buffer.push_str("->");
        if self.is_empty() {
          buffer.push_str(" ()");
        } else {
          buffer.push_str(" (\n");
          self.pretty_form_children(buffer, indent);
          buffer.push_str(pad.as_str());
          buffer.push(')');
        }

        if let Some(result) = &self.result {
          buffer.push_str(" ~ ");
          buffer.push_str(result.to_string().as_str());
        }
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

        if let Some(result) = &self.result {
          buffer.push('~');
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::VALUE(value) => {
        buffer.push_str(value.str_form().as_str());

        if let Some(result) = &self.result {
          buffer.push('~');
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::RESOLVE(str) => {
        buffer.push_str(str.to_string().as_str());

        if let Some(result) = &self.result {
          buffer.push('~');
          buffer.push_str(result.to_string().as_str());
        }
      }
      PlanNodeType::PIPELINE => {
        buffer.push_str("->");
        buffer.push('(');
        self.str_form_children(&mut buffer);
        buffer.push(')');

        if let Some(result) = &self.result {
          buffer.push('~');
          buffer.push_str(result.to_string().as_str());
        }
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
  pub fn container<S: Into<String>>(label: S) -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::CONTAINER(label.into()),
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
  pub fn value_node<T: Into<NodeValue>>(value: T) -> ExecutionPlanNode {
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

  /// Constructor for an apply node
  pub fn apply() -> ExecutionPlanNode {
    ExecutionPlanNode {
      node_type: PlanNodeType::PIPELINE,
      result: None,
      children: vec![],
    }
  }

  /// Adds the node as a child
  pub fn add<N>(&mut self, node: N) -> &mut Self where N: Into<ExecutionPlanNode> {
    self.children.push(node.into());
    self
  }

  /// Pushes the node onto the front of the list
  pub fn push_node(&mut self, node: ExecutionPlanNode) {
    self.children.insert(0, node.into());
  }

  /// If the node is a leaf node
  pub fn is_empty(&self) -> bool {
    match self.node_type {
      PlanNodeType::EMPTY => true,
      _ => self.children.is_empty()
    }
  }

  /// Returns the value for the node
  pub fn value(&self) -> Option<NodeResult> {
    self.result.clone()
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
  /// Root node for the plan tree
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
    .add(ExecutionPlanNode::value_node(expected.method.as_str()));

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
        .add(ExecutionPlanNode::value_node(expected.path.as_str()))
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
            .add(ExecutionPlanNode::resolve_value(DocPath::new("$.content-type")?))
            .add(ExecutionPlanNode::value_node(content_type.to_string()))
        );
      if let Some(plan_builder) = get_body_plan_builder(&content_type) {
        content_type_check_node.add(plan_builder.build_plan(content, context)?);
      } else {
        let plan_builder = PlainTextBuilder::new();
        content_type_check_node.add(plan_builder.build_plan(content, context)?);
      }
      plan_node.add(content_type_check_node);
    }
  }

  Ok(plan_node)
}

/// Executes the request plan against the actual request.
pub fn execute_request_plan(
  plan: &ExecutionPlan,
  actual: &HttpRequest,
  context: &mut PlanMatchingContext
) -> anyhow::Result<ExecutionPlan> {
  let value_resolver = HttpRequestValueResolver {
    request: actual.clone()
  };
  let path = vec![];
  let executed_tree = walk_tree(&path, &plan.plan_root, &value_resolver, context)?;
  Ok(ExecutionPlan {
    plan_root: executed_tree
  })
}

fn walk_tree(
  path: &[String],
  node: &ExecutionPlanNode,
  value_resolver: &dyn ValueResolver,
  context: &mut PlanMatchingContext
) -> anyhow::Result<ExecutionPlanNode> {
  match &node.node_type {
    PlanNodeType::EMPTY => {
      trace!(?path, "Empty node");
      Ok(node.clone())
    },
    PlanNodeType::CONTAINER(label) => {
      trace!(?path, %label, "Container node");
      let mut result = vec![];

      let mut child_path = path.to_vec();
      child_path.push(label.clone());
      let mut status = NodeResult::OK;
      for child in &node.children {
        let child_result = walk_tree(&child_path, child, value_resolver, context)?;
        status = status.or(&child_result.result);
        result.push(child_result);
      }

      Ok(ExecutionPlanNode {
        node_type: node.node_type.clone(),
        result: Some(status),
        children: result
      })
    }
    PlanNodeType::ACTION(action) => {
      trace!(?path, %action, "Action node");

      let mut child_path = path.to_vec();
      child_path.push(action.clone());
      let mut result = vec![];
      for child in &node.children {
        let child_result = if child.result.is_none() {
          walk_tree(&child_path, child, value_resolver, context)?
        } else {
          child.clone()
        };
        result.push(child_result);
      }
      match context.execute_action(action.as_str(), &result) {
        Ok(val) => {
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(val.clone()),
            children: result.clone()
          })
        }
        Err(err) => {
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(err.to_string())),
            children: result.clone()
          })
        }
      }
    }
    PlanNodeType::VALUE(val) => {
      trace!(?path, ?val, "Value node");
      Ok(ExecutionPlanNode {
        node_type: node.node_type.clone(),
        result: Some(NodeResult::VALUE(val.clone())),
        children: vec![]
      })
    }
    PlanNodeType::RESOLVE(resolve_path) => {
      match value_resolver.resolve(resolve_path, context) {
        Ok(val) => {
          trace!(?path, %resolve_path, ?val, "Resolve node");
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::VALUE(val.clone())),
            children: vec![]
          })
        }
        Err(err) => {
          trace!(?path, %resolve_path, %err, "Resolve node failed");
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR(err.to_string())),
            children: vec![]
          })
        }
      }
    }
    PlanNodeType::PIPELINE => {
      trace!(?path, "Apply pipeline node");

      let child_path = path.to_vec();
      context.push_result(None);

      for child in &node.children {
        let child_result = walk_tree(&child_path, child, value_resolver, context)?;
        context.update_result(child_result.result);
      }

      let result = context.pop_result();
      match result {
        Some(value) => {
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(value),
            children: vec![]
          })
        }
        None => {
          trace!(?path, "Value from stack is empty");
          Ok(ExecutionPlanNode {
            node_type: node.node_type.clone(),
            result: Some(NodeResult::ERROR("Value from stack is empty".to_string())),
            children: vec![]
          })
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use expectest::prelude::*;
  use pretty_assertions::assert_eq;
  use rstest::rstest;
  use serde_json::json;

  use pact_models::bodies::OptionalBody;
  use pact_models::content_types::TEXT;
  use pact_models::v4::http_parts::HttpRequest;

  use crate::engine::{build_request_plan, execute_request_plan, NodeResult, NodeValue, PlanMatchingContext};

  #[rstest(
    case("", "''"),
    case("simple", "'simple'"),
    case("simple sentence", "'simple sentence'"),
    case("\"quoted sentence\"", "'\"quoted sentence\"'"),
    case("'quoted sentence'", "\"'quoted sentence'\""),
    case("new\nline", "\"new\\nline\""),
  )]
  fn node_value_str_form_escapes_strings(#[case] input: &str, #[case] expected: &str) {
    let node = NodeValue::STRING(input.to_string());
    expect!(node.str_form()).to(be_equal_to(expected));
  }

  #[rstest(
    case(NodeResult::OK, None, NodeResult::OK),
    case(NodeResult::VALUE(NodeValue::NULL), None, NodeResult::VALUE(NodeValue::NULL)),
    case(NodeResult::ERROR("".to_string()), None, NodeResult::ERROR("".to_string())),
    case(NodeResult::OK, Some(NodeResult::OK), NodeResult::OK),
    case(NodeResult::OK, Some(NodeResult::VALUE(NodeValue::NULL)), NodeResult::OK),
    case(NodeResult::OK, Some(NodeResult::ERROR("".to_string())), NodeResult::ERROR("One or more children failed".to_string())),
    case(NodeResult::VALUE(NodeValue::NULL), Some(NodeResult::OK), NodeResult::OK),
    case(NodeResult::VALUE(NodeValue::NULL), Some(NodeResult::VALUE(NodeValue::NULL)), NodeResult::OK),
    case(NodeResult::VALUE(NodeValue::NULL), Some(NodeResult::ERROR("".to_string())), NodeResult::ERROR("One or more children failed".to_string())),
    case(NodeResult::ERROR("".to_string()), Some(NodeResult::OK), NodeResult::ERROR("One or more children failed".to_string())),
    case(NodeResult::ERROR("".to_string()), Some(NodeResult::VALUE(NodeValue::NULL)), NodeResult::ERROR("One or more children failed".to_string())),
    case(NodeResult::ERROR("".to_string()), Some(NodeResult::ERROR("".to_string())), NodeResult::ERROR("One or more children failed".to_string())),
  )]
  fn node_result_or(#[case] a: NodeResult, #[case] b: Option<NodeResult>, #[case] result: NodeResult) {
    expect!(a.or(&b)).to(be_equal_to(result));
  }

  #[test_log::test]
  fn simple_match_request_test() -> anyhow::Result<()> {
    let request = HttpRequest {
      method: "put".to_string(),
      path: "/test".to_string(),
      body: OptionalBody::Present("Some nice bit of text".into(), Some(TEXT.clone()), None),
      .. Default::default()
    };
    let expected_request = HttpRequest {
      method: "POST".to_string(),
      path: "/test".to_string(),
      query: None,
      headers: None,
      body: OptionalBody::Present("Some nice bit of text".into(), Some(TEXT.clone()), None),
      .. Default::default()
    };
    let mut context = PlanMatchingContext::default();
    let plan = build_request_plan(&expected_request, &context)?;

    assert_eq!(plan.pretty_form(),
r#"(
  :request (
    :method (
      %match:equality (
        %upper-case (
          $.method
        ),
        'POST'
      )
    ),
    :path (
      %match:equality (
        $.path,
        '/test'
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
          $.content-type,
          'text/plain'
        ),
        %match:equality (
          %convert:UTF8 (
            $.body
          ),
          'Some nice bit of text'
        )
      )
    )
  )
)
"#);

    let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
    assert_eq!(executed_plan.pretty_form(),
r#"(
  :request (
    :method (
      %match:equality (
        %upper-case (
          $.method ~ 'put'
        ) ~ 'PUT',
        'POST' ~ 'POST'
      ) ~ ERROR(Expected 'PUT' to equal 'POST')
    ),
    :path (
      %match:equality (
        $.path ~ '/test',
        '/test' ~ '/test'
      ) ~ BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query ~ {}
      ) ~ BOOL(true)
    ),
    :body (
      %if (
        %match:equality (
          $.content-type ~ 'text/plain',
          'text/plain' ~ 'text/plain'
        ) ~ BOOL(true),
        %match:equality (
          %convert:UTF8 (
            $.body ~ BYTES(21, U29tZSBuaWNlIGJpdCBvZiB0ZXh0)
          ) ~ 'Some nice bit of text',
          'Some nice bit of text' ~ 'Some nice bit of text'
        ) ~ BOOL(true)
      ) ~ BOOL(true)
    )
  )
)
"#);

    Ok(())
  }

  #[test_log::test]
  fn simple_json_match_request_test() -> anyhow::Result<()> {
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
    let mut context = PlanMatchingContext::default();
    let plan = build_request_plan(&expected_request, &context)?;

    assert_eq!(plan.pretty_form(),
r#"(
  :request (
    :method (
      %match:equality (
        %upper-case (
          $.method
        ),
        'POST'
      )
    ),
    :path (
      %match:equality (
        $.path,
        '/test'
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
          'application/json;charset=utf-8'
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

    let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
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
