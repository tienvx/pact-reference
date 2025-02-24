use bytes::Bytes;
use googletest::prelude::*;
use serde_json::{json, Value};
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
      $.body => BYTES(4, bnVsbA==)
    ) => json:null,
    %match:equality (
      json:null => json:null,
      %apply () => json:null,
      NULL => NULL
    ) => BOOL(true)
  ) => BOOL(true)");

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
      $.body => BYTES(4, dHJ1ZQ==)
    ) => json:true,
    %match:equality (
      json:null => json:null,
      %apply () => json:true,
      NULL => NULL
    ) => ERROR(Expected true (Boolean) to be equal to null (Null))
  ) => ERROR(Expected true (Boolean) to be equal to null (Null))");

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
      $.body => BYTES(1, ew==)
    ) => ERROR(json parse error - EOF while parsing an object at line 1 column 1),
    %match:equality (
      json:null => json:null,
      %apply () => ERROR(json parse error - EOF while parsing an object at line 1 column 1),
      NULL => NULL
    ) => ERROR(json parse error - EOF while parsing an object at line 1 column 1)
  ) => ERROR(json parse error - EOF while parsing an object at line 1 column 1)");
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
      $.body => BYTES(4, dHJ1ZQ==)
    ) => json:true,
    %match:equality (
      json:true => json:true,
      %apply () => json:true,
      NULL => NULL
    ) => BOOL(true)
  ) => BOOL(true)");

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
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    %match:equality (
      json:true => json:true,
      %apply () => json:false,
      NULL => NULL
    ) => ERROR(Expected false (Boolean) to be equal to true (Boolean))
  ) => ERROR(Expected false (Boolean) to be equal to true (Boolean))");
}

#[test_log::test]
fn json_with_empty_array() {
  let path = vec!["$".to_string()];
  let builder = JsonPlanBuilder::new();
  let mut context = PlanMatchingContext::default();
  let content = Bytes::copy_from_slice(Value::Array(vec![]).to_string().as_bytes());
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
      $.body => BYTES(2, W10=)
    ) => json:[],
    %json:expect:empty (
      'ARRAY' => 'ARRAY',
      %apply () => json:[]
    ) => BOOL(true)
  ) => BOOL(true)");

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
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    %json:expect:empty (
      'ARRAY' => 'ARRAY',
      %apply () => json:false
    ) => ERROR(Was expecting a JSON Array but got a Boolean)
  ) => ERROR(Was expecting a JSON Array but got a Boolean)");

  let content = Bytes::copy_from_slice(Value::Array(vec![Value::Bool(true)]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body => BYTES(6, W3RydWVd)
    ) => json:[true],
    %json:expect:empty (
      'ARRAY' => 'ARRAY',
      %apply () => json:[true]
    ) => ERROR(Expected JSON Array ([true]) to be empty)
  ) => ERROR(Expected JSON Array ([true]) to be empty)");
}

#[test_log::test]
fn json_with_array() {
  let path = vec!["$".to_string()];
  let builder = JsonPlanBuilder::new();
  let mut context = PlanMatchingContext::default();
  let content = Bytes::copy_from_slice(json!([1, 2, 3]).to_string().as_bytes());
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
      $.body => BYTES(7, WzEsMiwzXQ==)
    ) => json:[1,2,3],
    %push () => json:[1,2,3],
    %json:match:length (
      'ARRAY' => 'ARRAY',
      UINT(3) => UINT(3),
      %apply () => json:[1,2,3]
    ) => BOOL(true),
    %pop () => json:[1,2,3],
    :$ (
      :$[0] (
        %if (
          %check:exists (
            ~>$[0] => json:1
          ) => BOOL(true),
          %match:equality (
            json:1 => json:1,
            ~>$[0] => json:1,
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :$[1] (
        %if (
          %check:exists (
            ~>$[1] => json:2
          ) => BOOL(true),
          %match:equality (
            json:2 => json:2,
            ~>$[1] => json:2,
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :$[2] (
        %if (
          %check:exists (
            ~>$[2] => json:3
          ) => BOOL(true),
          %match:equality (
            json:3 => json:3,
            ~>$[2] => json:3,
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      )
    )
  ) => BOOL(true)");

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
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    %push () => json:false,
    %json:match:length (
      'ARRAY' => 'ARRAY',
      UINT(3) => UINT(3),
      %apply () => json:false
    ) => ERROR(Was expecting a JSON Array but got a Boolean),
    %pop () => json:false,
    :$ (
      :$[0] (
        %if (
          %check:exists (
            ~>$[0] => NULL
          ) => BOOL(false),
          %match:equality (
            json:1,
            ~>$[0],
            NULL
          )
        ) => BOOL(false)
      ),
      :$[1] (
        %if (
          %check:exists (
            ~>$[1] => NULL
          ) => BOOL(false),
          %match:equality (
            json:2,
            ~>$[1],
            NULL
          )
        ) => BOOL(false)
      ),
      :$[2] (
        %if (
          %check:exists (
            ~>$[2] => NULL
          ) => BOOL(false),
          %match:equality (
            json:3,
            ~>$[2],
            NULL
          )
        ) => BOOL(false)
      )
    )
  ) => BOOL(false)");

  let content = Bytes::copy_from_slice(Value::Array(vec![Value::Bool(true)]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body => BYTES(6, W3RydWVd)
    ) => json:[true],
    %push () => json:[true],
    %json:match:length (
      'ARRAY' => 'ARRAY',
      UINT(3) => UINT(3),
      %apply () => json:[true]
    ) => ERROR(Was expecting a length of 3, but actual length is 1),
    %pop () => json:[true],
    :$ (
      :$[0] (
        %if (
          %check:exists (
            ~>$[0] => json:true
          ) => BOOL(true),
          %match:equality (
            json:1 => json:1,
            ~>$[0] => json:true,
            NULL => NULL
          ) => ERROR(Expected true (Boolean) to be equal to 1 (Integer))
        ) => ERROR(Expected true (Boolean) to be equal to 1 (Integer))
      ),
      :$[1] (
        %if (
          %check:exists (
            ~>$[1] => NULL
          ) => BOOL(false),
          %match:equality (
            json:2,
            ~>$[1],
            NULL
          )
        ) => BOOL(false)
      ),
      :$[2] (
        %if (
          %check:exists (
            ~>$[2] => NULL
          ) => BOOL(false),
          %match:equality (
            json:3,
            ~>$[2],
            NULL
          )
        ) => BOOL(false)
      )
    )
  ) => ERROR(One or more children failed)");

  let content = Bytes::copy_from_slice(json!([1, 3, 3]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!(buffer,
  "  -> (
    %json:parse (
      $.body => BYTES(7, WzEsMywzXQ==)
    ) => json:[1,3,3],
    %push () => json:[1,3,3],
    %json:match:length (
      'ARRAY' => 'ARRAY',
      UINT(3) => UINT(3),
      %apply () => json:[1,3,3]
    ) => BOOL(true),
    %pop () => json:[1,3,3],
    :$ (
      :$[0] (
        %if (
          %check:exists (
            ~>$[0] => json:1
          ) => BOOL(true),
          %match:equality (
            json:1 => json:1,
            ~>$[0] => json:1,
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :$[1] (
        %if (
          %check:exists (
            ~>$[1] => json:3
          ) => BOOL(true),
          %match:equality (
            json:2 => json:2,
            ~>$[1] => json:3,
            NULL => NULL
          ) => ERROR(Expected 3 (Integer) to be equal to 2 (Integer))
        ) => ERROR(Expected 3 (Integer) to be equal to 2 (Integer))
      ),
      :$[2] (
        %if (
          %check:exists (
            ~>$[2] => json:3
          ) => BOOL(true),
          %match:equality (
            json:3 => json:3,
            ~>$[2] => json:3,
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      )
    )
  ) => ERROR(One or more children failed)");
}
