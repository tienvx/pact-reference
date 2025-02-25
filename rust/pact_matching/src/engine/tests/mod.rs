use expectest::prelude::*;
use maplit::hashmap;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use pact_models::bodies::OptionalBody;
use pact_models::content_types::TEXT;
use pact_models::matchingrules;
use pact_models::v4::http_parts::HttpRequest;
use pact_models::v4::interaction::V4Interaction;
use pact_models::v4::synch_http::SynchronousHttp;
use crate::engine::{build_request_plan, execute_request_plan, ExecutionPlan, NodeResult, NodeValue, PlanMatchingContext, setup_query_plan};
use crate::MatchingRule;

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
  case(NodeResult::OK, Some(NodeResult::VALUE(NodeValue::NULL)), NodeResult::VALUE(NodeValue::NULL)),
  case(NodeResult::OK, Some(NodeResult::ERROR("".to_string())), NodeResult::ERROR("One or more children failed".to_string())),
  case(NodeResult::VALUE(NodeValue::NULL), Some(NodeResult::OK), NodeResult::VALUE(NodeValue::NULL)),
  case(NodeResult::VALUE(NodeValue::NULL), Some(NodeResult::VALUE(NodeValue::NULL)), NodeResult::VALUE(NodeValue::NULL)),
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

  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'POST',
        %upper-case (
          $.method
        ),
        NULL
      )
    ),
    :path (
      %match:equality (
        '/test',
        $.path,
        NULL
      )
    ),
    :"query parameters" (
      %expect:empty (
        $.query,
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      )
    ),
    :body (
      %if (
        %match:equality (
          'text/plain',
          $.content-type,
          NULL
        ),
        %match:equality (
          'Some nice bit of text',
          %convert:UTF8 (
            $.body
          ),
          NULL
        )
      )
    )
  )
)
"#, plan.pretty_form());

  let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'POST' => 'POST',
        %upper-case (
          $.method => 'put'
        ) => 'PUT',
        NULL => NULL
      ) => ERROR(Expected 'PUT' to be equal to 'POST')
    ),
    :path (
      %match:equality (
        '/test' => '/test',
        $.path => '/test',
        NULL => NULL
      ) => BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {},
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      ) => BOOL(true)
    ),
    :body (
      %if (
        %match:equality (
          'text/plain' => 'text/plain',
          $.content-type => 'text/plain',
          NULL => NULL
        ) => BOOL(true),
        %match:equality (
          'Some nice bit of text' => 'Some nice bit of text',
          %convert:UTF8 (
            $.body => BYTES(21, U29tZSBuaWNlIGJpdCBvZiB0ZXh0)
          ) => 'Some nice bit of text',
          NULL => NULL
        ) => BOOL(true)
      ) => BOOL(true)
    )
  )
)
"#, executed_plan.pretty_form());

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

  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'POST',
        %upper-case (
          $.method
        ),
        NULL
      )
    ),
    :path (
      %match:equality (
        '/test',
        $.path,
        NULL
      )
    ),
    :"query parameters" (
      %expect:empty (
        $.query,
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      )
    ),
    :body (
      %if (
        %match:equality (
          'application/json;charset=utf-8',
          $.content-type,
          NULL
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
                json:100,
                ~>$.a,
                NULL
              )
            ),
            :$.b (
              %match:equality (
                json:200.1,
                ~>$.b,
                NULL
              )
            )
          )
        )
      )
    )
  )
)
"#, plan.pretty_form());

  let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'POST' => 'POST',
        %upper-case (
          $.method => 'POST'
        ) => 'POST',
        NULL => NULL
      ) => BOOL(true)
    ),
    :path (
      %match:equality (
        '/test' => '/test',
        $.path => '/test',
        NULL => NULL
      ) => BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {},
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      ) => BOOL(true)
    ),
    :body (
      %if (
        %match:equality (
          'application/json;charset=utf-8' => 'application/json;charset=utf-8',
          $.content-type => 'application/json;charset=utf-8',
          NULL => NULL
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
                json:100 => json:100,
                ~>$.a => NULL,
                NULL => NULL
              ) => ERROR(Expected null (Null) to be equal to 100 (Integer))
            ),
            :$.b (
              %match:equality (
                json:200.1 => json:200.1,
                ~>$.b => json:"22",
                NULL => NULL
              ) => ERROR(Expected '22' (String) to be equal to 200.1 (Decimal))
            )
          )
        ) => ERROR(One or more children failed)
      ) => ERROR(One or more children failed)
    )
  )
)
"#, executed_plan.pretty_form());

  Ok(())
}

#[test_log::test]
fn match_path_with_matching_rule() -> anyhow::Result<()> {
  let request = HttpRequest {
    method: "get".to_string(),
    path: "/test12345".to_string(),
    .. Default::default()
  };
  let matching_rules = matchingrules! {
    "path" => { "" => [ MatchingRule::Regex("\\/test[0-9]+".to_string()) ] }
  };
  let expected_request = HttpRequest {
    method: "get".to_string(),
    path: "/test".to_string(),
    matching_rules: matching_rules.clone(),
    .. Default::default()
  };
  let expected_interaction = SynchronousHttp {
    request: expected_request.clone(),
    .. SynchronousHttp::default()
  };
  let mut context = PlanMatchingContext {
    interaction: expected_interaction.boxed_v4(),
    .. PlanMatchingContext::default()
  };
  let plan = build_request_plan(&expected_request, &context)?;

  assert_eq!(
r#"(
  :request (
    :method (
      %match:equality (
        'GET',
        %upper-case (
          $.method
        ),
        NULL
      )
    ),
    :path (
      %match:regex (
        '/test',
        $.path,
        json:{"regex":"\\/test[0-9]+"}
      )
    ),
    :"query parameters" (
      %expect:empty (
        $.query,
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      )
    )
  )
)
"#, plan.pretty_form());

  let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'GET' => 'GET',
        %upper-case (
          $.method => 'get'
        ) => 'GET',
        NULL => NULL
      ) => BOOL(true)
    ),
    :path (
      %match:regex (
        '/test' => '/test',
        $.path => '/test12345',
        json:{"regex":"\\/test[0-9]+"} => json:{"regex":"\\/test[0-9]+"}
      ) => BOOL(true)
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {},
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      ) => BOOL(true)
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    method: "get".to_string(),
    path: "/test12345X".to_string(),
    .. Default::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context)?;
  assert_eq!(r#"(
  :request (
    :method (
      %match:equality (
        'GET' => 'GET',
        %upper-case (
          $.method => 'get'
        ) => 'GET',
        NULL => NULL
      ) => BOOL(true)
    ),
    :path (
      %match:regex (
        '/test' => '/test',
        $.path => '/test12345X',
        json:{"regex":"\\/test[0-9]+"} => json:{"regex":"\\/test[0-9]+"}
      ) => ERROR(Expected '/test12345X' to match '\/test[0-9]+')
    ),
    :"query parameters" (
      %expect:empty (
        $.query => {},
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      ) => BOOL(true)
    )
  )
)
"#, executed_plan.pretty_form());

  Ok(())
}

#[test]
fn match_query_with_no_query_strings() {
  let expected = HttpRequest::default();
  let mut context = PlanMatchingContext::default();
  let mut plan = ExecutionPlan::new("query-test");

  plan.add(setup_query_plan(&expected, &context.for_query()).unwrap());
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      %expect:empty (
        $.query,
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      )
    )
  )
)
"#, plan.pretty_form());

  let request = HttpRequest::default();
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      %expect:empty (
        $.query => {},
        %join (
          'Expected no query parameters but got ',
          $.query
        )
      ) => BOOL(true)
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    query: Some(hashmap!{
      "a".to_string() => vec![Some("b".to_string())]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      %expect:empty (
        $.query => {'a': 'b'},
        %join (
          'Expected no query parameters but got ' => 'Expected no query parameters but got ',
          $.query => {'a': 'b'}
        ) => "Expected no query parameters but got {'a': 'b'}"
      ) => ERROR(Expected no query parameters but got {'a': 'b'})
    )
  )
)
"#, executed_plan.pretty_form());
}

#[test]
fn match_query_with_expected_query_string() {
  let expected = HttpRequest {
    query: Some(hashmap!{
      "a".to_string() => vec![Some("b".to_string())]
    }),
    .. HttpRequest::default()
  };
  let mut context = PlanMatchingContext::default();
  let mut plan = ExecutionPlan::new("query-test");

  plan.add(setup_query_plan(&expected, &context.for_query()).unwrap());
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a
          ),
          %match:equality (
            'b',
            $.query.a,
            NULL
          )
        )
      ),
      %expect:entries (
        ['a'],
        $.query,
        %join (
          'The following expected query parameters were missing: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ),
      %expect:only-entries (
        ['a'],
        $.query,
        %join (
          'The following query parameters were not expected: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      )
    )
  )
)
"#, plan.pretty_form());

  let request = HttpRequest::default();
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a => NULL
          ) => BOOL(false),
          %match:equality (
            'b',
            $.query.a,
            NULL
          )
        ) => BOOL(false)
      ),
      %expect:entries (
        ['a'] => ['a'],
        $.query => {},
        %join (
          'The following expected query parameters were missing: ' => 'The following expected query parameters were missing: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'a'
            ) => OK
          ) => 'a'
        ) => 'The following expected query parameters were missing: a'
      ) => ERROR(The following expected query parameters were missing: a),
      %expect:only-entries (
        ['a'] => ['a'],
        $.query => {},
        %join (
          'The following query parameters were not expected: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    query: Some(hashmap!{
      "a".to_string() => vec![Some("b".to_string())]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a => 'b'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.query.a => 'b',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['a'] => ['a'],
        $.query => {'a': 'b'},
        %join (
          'The following expected query parameters were missing: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK,
      %expect:only-entries (
        ['a'] => ['a'],
        $.query => {'a': 'b'},
        %join (
          'The following query parameters were not expected: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    query: Some(hashmap!{
      "a".to_string() => vec![Some("c".to_string())]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a => 'c'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.query.a => 'c',
            NULL => NULL
          ) => ERROR(Expected 'c' to be equal to 'b')
        ) => ERROR(Expected 'c' to be equal to 'b')
      ),
      %expect:entries (
        ['a'] => ['a'],
        $.query => {'a': 'c'},
        %join (
          'The following expected query parameters were missing: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK,
      %expect:only-entries (
        ['a'] => ['a'],
        $.query => {'a': 'c'},
        %join (
          'The following query parameters were not expected: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    query: Some(hashmap!{
      "a".to_string() => vec![Some("b".to_string())],
      "b".to_string() => vec![Some("c".to_string())]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a => 'b'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.query.a => 'b',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['a'] => ['a'],
        $.query => {'a': 'b', 'b': 'c'},
        %join (
          'The following expected query parameters were missing: ',
          %join-with (
            ', ',
            ** (
              %apply ()
            )
          )
        )
      ) => OK,
      %expect:only-entries (
        ['a'] => ['a'],
        $.query => {'a': 'b', 'b': 'c'},
        %join (
          'The following query parameters were not expected: ' => 'The following query parameters were not expected: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'b'
            ) => OK
          ) => 'b'
        ) => 'The following query parameters were not expected: b'
      ) => ERROR(The following query parameters were not expected: b)
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    query: Some(hashmap!{
      "b".to_string() => vec![Some("c".to_string())]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  assert_eq!(r#"(
  :query-test (
    :"query parameters" (
      :$.query.a (
        %if (
          %check:exists (
            $.query.a => NULL
          ) => BOOL(false),
          %match:equality (
            'b',
            $.query.a,
            NULL
          )
        ) => BOOL(false)
      ),
      %expect:entries (
        ['a'] => ['a'],
        $.query => {'b': 'c'},
        %join (
          'The following expected query parameters were missing: ' => 'The following expected query parameters were missing: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'a'
            ) => OK
          ) => 'a'
        ) => 'The following expected query parameters were missing: a'
      ) => ERROR(The following expected query parameters were missing: a),
      %expect:only-entries (
        ['a'] => ['a'],
        $.query => {'b': 'c'},
        %join (
          'The following query parameters were not expected: ' => 'The following query parameters were not expected: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'b'
            ) => OK
          ) => 'b'
        ) => 'The following query parameters were not expected: b'
      ) => ERROR(The following query parameters were not expected: b)
    )
  )
)
"#, executed_plan.pretty_form());
}
