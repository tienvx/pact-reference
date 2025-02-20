use expectest::prelude::*;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use pact_models::bodies::OptionalBody;
use pact_models::content_types::TEXT;
use pact_models::v4::http_parts::HttpRequest;

use crate::engine::{build_request_plan, execute_request_plan, NodeResult, NodeValue, PlanMatchingContext};

mod walk_tree_tests;

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
          $.method => 'put'
        ) => 'PUT',
        'POST' => 'POST'
      ) => ERROR(Expected 'PUT' to equal 'POST')
    ),
    :path (
      %match:equality (
        $.path => '/test',
        '/test' => '/test'
      ) => BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {}
      ) => BOOL(true)
    ),
    :body (
      %if (
        %match:equality (
          $.content-type => 'text/plain',
          'text/plain' => 'text/plain'
        ) => BOOL(true),
        %match:equality (
          %convert:UTF8 (
            $.body => BYTES(21, U29tZSBuaWNlIGJpdCBvZiB0ZXh0)
          ) => 'Some nice bit of text',
          'Some nice bit of text' => 'Some nice bit of text'
        ) => BOOL(true)
      ) => BOOL(true)
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
          $.content-type,
          'application/json;charset=utf-8'
        ),
        -> (
          %json:parse (
            $.body
          ),
          %push (),
          %json:expect:entries (
            'OBJECT',
            ['a', 'b'],
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
                json:200.1
              )
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
          $.method => 'POST'
        ) => 'POST',
        'POST' => 'POST'
      ) => BOOL(true)
    ),
    :path (
      %match:equality (
        $.path => '/test',
        '/test' => '/test'
      ) => BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {}
      ) => BOOL(true)
    ),
    :body (
      %if (
        %match:equality (
          $.content-type => 'application/json;charset=utf-8',
          'application/json;charset=utf-8' => 'application/json;charset=utf-8'
        ) => BOOL(true),
        -> (
          %json:parse (
            $.body => BYTES(10, eyJiIjoiMjIifQ==)
          ) => json:{"b":"22"},
          %push () => json:{"b":"22"},
          %json:expect:entries (
            'OBJECT' => 'OBJECT',
            ['a', 'b'] => ['a', 'b'],
            %apply () => json:{"b":"22"}
          ) => ERROR(The following expected entries were missing from the actual Object: a),
          %pop () => json:{"b":"22"},
          :$ (
            :$.a (
              %match:equality (
                ~>$.a => NULL,
                json:100 => json:100
              ) => ERROR(Expected NULL to equal json:100)
            ),
            :$.b (
              %match:equality (
                ~>$.b => json:"22",
                json:200.1 => json:200.1
              ) => ERROR(Expected json:"22" to equal json:200.1)
            )
          )
        ) => ERROR(One or more children failed)
      ) => ERROR(One or more children failed)
    )
  )
)
"#);

  Ok(())
}
