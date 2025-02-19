use anyhow::anyhow;
use bytes::Bytes;
use googletest::prelude::*;
use serde_json::Value;
use tracing::trace;
use pact_models::path_exp::DocPath;
use pretty_assertions::assert_eq;

use crate::engine::bodies::{JsonPlanBuilder, PlanBodyBuilder};
use crate::engine::context::PlanMatchingContext;
use crate::engine::NodeValue;
use crate::engine::value_resolvers::ValueResolver;
use crate::engine::walk_tree;

struct TestValueResolver {
  pub bytes: Vec<u8>
}

impl ValueResolver for TestValueResolver {
  fn resolve(&self, path: &DocPath, _context: &PlanMatchingContext) -> anyhow::Result<NodeValue> {
    trace!(%path, "resolve called");
    Ok(NodeValue::BARRAY(self.bytes.clone()))
  }
}

#[test_log::test]
fn json_with_null() {
  let path = vec!["$".to_string()];
  let builder = JsonPlanBuilder::new();
  let mut context = PlanMatchingContext::default();
  let content = Bytes::copy_from_slice(Value::Null.to_string().as_bytes());
  let node = builder.build_plan(&content, &context).unwrap();

  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body ~ BYTES(4, bnVsbA==)
    ) ~ json:null,
    %match:equality (
      json:null ~ json:null,
      %apply () ~ json:null
    ) ~ BOOL(true)
  ) ~ BOOL(true)");

  let content = Bytes::copy_from_slice(Value::Bool(true).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body ~ BYTES(4, dHJ1ZQ==)
    ) ~ json:true,
    %match:equality (
      json:null ~ json:null,
      %apply () ~ json:true
    ) ~ ERROR(Expected json:null to equal json:true)
  ) ~ ERROR(Expected json:null to equal json:true)");

  let content = Bytes::copy_from_slice("{".as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body ~ BYTES(1, ew==)
    ) ~ ERROR(EOF while parsing an object at line 1 column 1),
    %match:equality (
      json:null ~ json:null,
      %apply () ~ ERROR(EOF while parsing an object at line 1 column 1)
    ) ~ ERROR(Expected json:null to equal NULL)
  ) ~ ERROR(Expected json:null to equal NULL)");
}

#[test_log::test]
fn json_with_boolean() {
  let path = vec!["$".to_string()];
  let builder = JsonPlanBuilder::new();
  let mut context = PlanMatchingContext::default();
  let content = Bytes::copy_from_slice(Value::Bool(true).to_string().as_bytes());
  let node = builder.build_plan(&content, &context).unwrap();

  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body ~ BYTES(4, dHJ1ZQ==)
    ) ~ json:true,
    %match:equality (
      json:true ~ json:true,
      %apply () ~ json:true
    ) ~ BOOL(true)
  ) ~ BOOL(true)");

  let content = Bytes::copy_from_slice(Value::Bool(false).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body ~ BYTES(5, ZmFsc2U=)
    ) ~ json:false,
    %match:equality (
      json:true ~ json:true,
      %apply () ~ json:false
    ) ~ ERROR(Expected json:true to equal json:false)
  ) ~ ERROR(Expected json:true to equal json:false)");
}
