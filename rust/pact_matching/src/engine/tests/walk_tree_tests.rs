use bytes::Bytes;
use serde_json::{json, Value};
use tracing::trace;
use pact_models::path_exp::DocPath;
use pretty_assertions::assert_eq;

use crate::engine::bodies::{JsonPlanBuilder, PlanBodyBuilder, XMLPlanBuilder};
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
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(4, bnVsbA==)
    ) => json:null,
    :$ (
      %match:equality (
        json:null => json:null,
        ~>$ => json:null,
        NULL => NULL
      ) => BOOL(true)
    ) => BOOL(true)
  ) => BOOL(true)", buffer);

  let content = Bytes::copy_from_slice(Value::Bool(true).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(4, dHJ1ZQ==)
    ) => json:true,
    :$ (
      %match:equality (
        json:null => json:null,
        ~>$ => json:true,
        NULL => NULL
      ) => ERROR(Expected true (Boolean) to be equal to null (Null))
    ) => BOOL(false)
  ) => BOOL(false)", buffer);

  let content = Bytes::copy_from_slice("{".as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(1, ew==)
    ) => ERROR(json parse error - EOF while parsing an object at line 1 column 1),
    :$ (
      %match:equality (
        json:null,
        ~>$,
        NULL
      )
    )
  ) => ERROR(json parse error - EOF while parsing an object at line 1 column 1)", buffer);
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
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(4, dHJ1ZQ==)
    ) => json:true,
    :$ (
      %match:equality (
        json:true => json:true,
        ~>$ => json:true,
        NULL => NULL
      ) => BOOL(true)
    ) => BOOL(true)
  ) => BOOL(true)", buffer);

  let content = Bytes::copy_from_slice(Value::Bool(false).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    :$ (
      %match:equality (
        json:true => json:true,
        ~>$ => json:false,
        NULL => NULL
      ) => ERROR(Expected false (Boolean) to be equal to true (Boolean))
    ) => BOOL(false)
  ) => BOOL(false)", buffer);
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
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(2, W10=)
    ) => json:[],
    :$ (
      %json:expect:empty (
        'ARRAY' => 'ARRAY',
        ~>$ => json:[]
      ) => BOOL(true)
    ) => BOOL(true)
  ) => BOOL(true)", buffer);

  let content = Bytes::copy_from_slice(Value::Bool(false).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    :$ (
      %json:expect:empty (
        'ARRAY' => 'ARRAY',
        ~>$ => json:false
      ) => ERROR(Was expecting a JSON Array but got a Boolean)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);

  let content = Bytes::copy_from_slice(Value::Array(vec![Value::Bool(true)]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(6, W3RydWVd)
    ) => json:[true],
    :$ (
      %json:expect:empty (
        'ARRAY' => 'ARRAY',
        ~>$ => json:[true]
      ) => ERROR(Expected JSON Array ([true]) to be empty)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);
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
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(7, WzEsMiwzXQ==)
    ) => json:[1,2,3],
    :$ (
      %json:match:length (
        'ARRAY' => 'ARRAY',
        UINT(3) => UINT(3),
        ~>$ => json:[1,2,3]
      ) => BOOL(true),
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
      ) => BOOL(true),
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
      ) => BOOL(true),
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
      ) => BOOL(true)
    ) => BOOL(true)
  ) => BOOL(true)", buffer);

  let content = Bytes::copy_from_slice(Value::Bool(false).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(5, ZmFsc2U=)
    ) => json:false,
    :$ (
      %json:match:length (
        'ARRAY' => 'ARRAY',
        UINT(3) => UINT(3),
        ~>$ => json:false
      ) => ERROR(Was expecting a JSON Array but got a Boolean),
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
      ) => BOOL(false),
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
      ) => BOOL(false),
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
      ) => BOOL(false)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);

  let content = Bytes::copy_from_slice(Value::Array(vec![Value::Bool(true)]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(6, W3RydWVd)
    ) => json:[true],
    :$ (
      %json:match:length (
        'ARRAY' => 'ARRAY',
        UINT(3) => UINT(3),
        ~>$ => json:[true]
      ) => ERROR(Was expecting a length of 3, but actual length is 1),
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
        ) => BOOL(false)
      ) => BOOL(false),
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
      ) => BOOL(false),
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
      ) => BOOL(false)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);

  let content = Bytes::copy_from_slice(json!([1, 3, 3]).to_string().as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %json:parse (
      $.body => BYTES(7, WzEsMywzXQ==)
    ) => json:[1,3,3],
    :$ (
      %json:match:length (
        'ARRAY' => 'ARRAY',
        UINT(3) => UINT(3),
        ~>$ => json:[1,3,3]
      ) => BOOL(true),
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
      ) => BOOL(true),
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
        ) => BOOL(false)
      ) => BOOL(false),
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
      ) => BOOL(true)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);
}

#[test_log::test]
fn simple_xml() {
  let path = vec!["$".to_string()];
  let builder = XMLPlanBuilder::new();
  let mut context = PlanMatchingContext::default();
  let content = Bytes::copy_from_slice("<foo>test</foo>".as_bytes());
  let node = builder.build_plan(&content, &context).unwrap();

  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %xml:parse (
      $.body => BYTES(15, PGZvbz50ZXN0PC9mb28+)
    ) => xml:'<foo>test</foo>',
    :$ (
      %expect:only-entries (
        ['foo'] => ['foo'],
        %xml:tag-name (
          ~>$ => xml:'<foo>test</foo>'
        ) => 'foo'
      ) => OK,
      :$.foo (
        %if (
          %check:exists (
            ~>$.foo => xml:'<foo>test</foo>'
          ) => BOOL(true),
          %match:equality (
            xml:'<foo>test</foo>' => xml:'<foo>test</foo>',
            ~>$.foo => xml:'<foo>test</foo>',
            NULL => NULL
          ) => BOOL(true),
          %error (
            'Was expecting an XML element <',
            %xml:tag-name (
              xml:'<foo>test</foo>'
            ),
            '> but it was missing'
          )
        ) => BOOL(true)
      ) => BOOL(true)
    ) => BOOL(true)
  ) => BOOL(true)", buffer);

  let content = Bytes::copy_from_slice("<bar></bar>".as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %xml:parse (
      $.body => BYTES(11, PGJhcj48L2Jhcj4=)
    ) => xml:'<bar/>',
    :$ (
      %expect:only-entries (
        ['foo'] => ['foo'],
        %xml:tag-name (
          ~>$ => xml:'<bar/>'
        ) => 'bar'
      ) => ERROR(The following unexpected entries were received: ['bar']),
      :$.foo (
        %if (
          %check:exists (
            ~>$.foo => NULL
          ) => BOOL(false),
          %match:equality (
            xml:'<foo>test</foo>',
            ~>$.foo,
            NULL
          ),
          %error (
            'Was expecting an XML element <' => 'Was expecting an XML element <',
            %xml:tag-name (
              xml:'<foo>test</foo>' => xml:'<foo>test</foo>'
            ) => 'foo',
            '> but it was missing' => '> but it was missing'
          ) => ERROR(Was expecting an XML element <foo> but it was missing)
        ) => ERROR(Was expecting an XML element <foo> but it was missing)
      ) => BOOL(false)
    ) => BOOL(false)
  ) => BOOL(false)", buffer);

  let content = Bytes::copy_from_slice("<foo>test".as_bytes());
  let resolver = TestValueResolver {
    bytes: content.to_vec()
  };
  let result = walk_tree(&path, &node, &resolver, &mut context).unwrap();
  let mut buffer = String::new();
  result.pretty_form(&mut buffer, 2);
  assert_eq!("  %tee (
    %xml:parse (
      $.body => BYTES(9, PGZvbz50ZXN0)
    ) => ERROR(XML parse error - ParsingError: root element not closed),
    :$ (
      %expect:only-entries (
        ['foo'],
        %xml:tag-name (
          ~>$
        )
      ),
      :$.foo (
        %if (
          %check:exists (
            ~>$.foo
          ),
          %match:equality (
            xml:'<foo>test</foo>',
            ~>$.foo,
            NULL
          ),
          %error (
            'Was expecting an XML element <',
            %xml:tag-name (
              xml:'<foo>test</foo>'
            ),
            '> but it was missing'
          )
        )
      )
    )
  ) => ERROR(XML parse error - ParsingError: root element not closed)", buffer);
}
