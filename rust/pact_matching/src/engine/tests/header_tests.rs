use maplit::hashmap;

use pact_models::matchingrules;
use pact_models::matchingrules::MatchingRule;
use pact_models::prelude::v4::SynchronousHttp;
use pact_models::v4::http_parts::HttpRequest;
use pact_models::v4::interaction::V4Interaction;

use crate::engine::{execute_request_plan, ExecutionPlan, setup_header_plan};
use crate::engine::context::PlanMatchingContext;

#[test]
fn match_headers_where_there_are_none() {
  let expected = HttpRequest::default();
  let mut context = PlanMatchingContext::default();
  let mut plan = ExecutionPlan::new("header-test");

  plan.add(setup_header_plan(&expected, &context.for_query()).unwrap());
  pretty_assertions::assert_eq!(r#"(
  :header-test ()
)
"#, plan.pretty_form());

  let request = HttpRequest::default();
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test ()
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    headers: Some(hashmap!{
      "HEADER_X".to_string() => vec!["A".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test ()
)
"#, executed_plan.pretty_form());
}

#[test]
fn match_query_with_expected_header() {
  let expected = HttpRequest {
    headers: Some(hashmap!{
      "HEADER-X".to_string() => vec!["b".to_string()]
    }),
    .. HttpRequest::default()
  };
  let mut context = PlanMatchingContext::default();
  let mut plan = ExecutionPlan::new("header-test");

  plan.add(setup_header_plan(&expected, &context.for_query()).unwrap());
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X']
          ),
          %match:equality (
            'b',
            $.headers['HEADER-X'],
            NULL
          )
        )
      ),
      %expect:entries (
        ['HEADER-X'],
        $.headers,
        %join (
          'The following expected headers were missing: ',
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
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X'] => NULL
          ) => BOOL(false),
          %match:equality (
            'b',
            $.headers['HEADER-X'],
            NULL
          )
        ) => BOOL(false)
      ),
      %expect:entries (
        ['HEADER-X'] => ['HEADER-X'],
        $.headers => {},
        %join (
          'The following expected headers were missing: ' => 'The following expected headers were missing: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'HEADER-X'
            ) => OK
          ) => 'HEADER-X'
        ) => 'The following expected headers were missing: HEADER-X'
      ) => ERROR(The following expected headers were missing: HEADER-X)
    )
  )
)
"#, executed_plan.pretty_form());

  let request = HttpRequest {
    headers: Some(hashmap!{
      "HEADER-X".to_string() => vec!["b".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X'] => 'b'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.headers['HEADER-X'] => 'b',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['HEADER-X'] => ['HEADER-X'],
        $.headers => {'HEADER-X': 'b'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "HEADER-X".to_string() => vec!["C".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X'] => 'C'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.headers['HEADER-X'] => 'C',
            NULL => NULL
          ) => ERROR(Expected 'C' to be equal to 'b')
        ) => ERROR(Expected 'C' to be equal to 'b')
      ),
      %expect:entries (
        ['HEADER-X'] => ['HEADER-X'],
        $.headers => {'HEADER-X': 'C'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "HEADER-X".to_string() => vec!["b".to_string()],
      "HEADER-Y".to_string() => vec!["b".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X'] => 'b'
          ) => BOOL(true),
          %match:equality (
            'b' => 'b',
            $.headers['HEADER-X'] => 'b',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['HEADER-X'] => ['HEADER-X'],
        $.headers => {'HEADER-X': 'b', 'HEADER-Y': 'b'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "HEADER-Y".to_string() => vec!["b".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :HEADER-X (
        %if (
          %check:exists (
            $.headers['HEADER-X'] => NULL
          ) => BOOL(false),
          %match:equality (
            'b',
            $.headers['HEADER-X'],
            NULL
          )
        ) => BOOL(false)
      ),
      %expect:entries (
        ['HEADER-X'] => ['HEADER-X'],
        $.headers => {'HEADER-Y': 'b'},
        %join (
          'The following expected headers were missing: ' => 'The following expected headers were missing: ',
          %join-with (
            ', ' => ', ',
            ** (
              %apply () => 'HEADER-X'
            ) => OK
          ) => 'HEADER-X'
        ) => 'The following expected headers were missing: HEADER-X'
      ) => ERROR(The following expected headers were missing: HEADER-X)
    )
  )
)
"#, executed_plan.pretty_form());
}

#[test]
fn match_headers_with_matching_rule() {
  let matching_rules = matchingrules! {
    "header" => { "REF-ID" => [ MatchingRule::Regex("^[0-9]+$".to_string()) ] }
  };
  let expected_request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
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

  let mut plan = ExecutionPlan::new("header-test");
  plan.add(setup_header_plan(&expected_request, &context.for_headers()).unwrap());

  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE']
          ),
          %match:equality (
            'test',
            $.headers['REF-CODE'],
            NULL
          )
        )
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID']
          ),
          %match:regex (
            '1234',
            $.headers['REF-ID'],
            json:{"regex":"^[0-9]+$"}
          )
        )
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'],
        $.headers,
        %join (
          'The following expected headers were missing: ',
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

  let request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["9023470945622".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '9023470945622'
          ) => BOOL(true),
          %match:regex (
            '1234' => '1234',
            $.headers['REF-ID'] => '9023470945622',
            json:{"regex":"^[0-9]+$"} => json:{"regex":"^[0-9]+$"}
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': '9023470945622'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["9023470X945622".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '9023470X945622'
          ) => BOOL(true),
          %match:regex (
            '1234' => '1234',
            $.headers['REF-ID'] => '9023470X945622',
            json:{"regex":"^[0-9]+$"} => json:{"regex":"^[0-9]+$"}
          ) => ERROR(Expected '9023470X945622' to match '^[0-9]+$')
        ) => ERROR(Expected '9023470X945622' to match '^[0-9]+$')
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': '9023470X945622'},
        %join (
          'The following expected headers were missing: ',
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
}

#[test]
fn match_headers_with_values_having_different_lengths() {
  let expected_request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string(), "test2".to_string()]
    }),
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

  let mut plan = ExecutionPlan::new("header-test");
  plan.add(setup_header_plan(&expected_request, &context.for_headers()).unwrap());

  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE']
          ),
          %match:equality (
            ['test', 'test2'],
            $.headers['REF-CODE'],
            NULL
          )
        )
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID']
          ),
          %match:equality (
            '1234',
            $.headers['REF-ID'],
            NULL
          )
        )
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'],
        $.headers,
        %join (
          'The following expected headers were missing: ',
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

  let request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string(), "test2".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => ['test', 'test2']
          ) => BOOL(true),
          %match:equality (
            ['test', 'test2'] => ['test', 'test2'],
            $.headers['REF-CODE'] => ['test', 'test2'],
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '1234'
          ) => BOOL(true),
          %match:equality (
            '1234' => '1234',
            $.headers['REF-ID'] => '1234',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': ['test', 'test2'], 'REF-ID': '1234'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string(), "1234".to_string(), "4567".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            ['test', 'test2'] => ['test', 'test2'],
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => ERROR(Expected ["test"] to be equal to ["test","test2"])
        ) => ERROR(Expected ["test"] to be equal to ["test","test2"])
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => ['1234', '1234', '4567']
          ) => BOOL(true),
          %match:equality (
            '1234' => '1234',
            $.headers['REF-ID'] => ['1234', '1234', '4567'],
            NULL => NULL
          ) => ERROR(Expected '4567' to be equal to '1234')
        ) => ERROR(Expected '4567' to be equal to '1234')
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': ['1234', '1234', '4567']},
        %join (
          'The following expected headers were missing: ',
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
}

#[test]
fn match_headers_with_number_type_matching_rule() {
  let matching_rules = matchingrules! {
    "header" => { "REF-ID" => [ MatchingRule::Integer ] }
  };
  let expected_request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
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

  let mut plan = ExecutionPlan::new("header-test");
  plan.add(setup_header_plan(&expected_request, &context.for_headers()).unwrap());

  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE']
          ),
          %match:equality (
            'test',
            $.headers['REF-CODE'],
            NULL
          )
        )
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID']
          ),
          %match:integer (
            '1234',
            $.headers['REF-ID'],
            json:{}
          )
        )
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'],
        $.headers,
        %join (
          'The following expected headers were missing: ',
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

  let request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["9023470945622".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '9023470945622'
          ) => BOOL(true),
          %match:integer (
            '1234' => '1234',
            $.headers['REF-ID'] => '9023470945622',
            json:{} => json:{}
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': '9023470945622'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["9023470X945622".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '9023470X945622'
          ) => BOOL(true),
          %match:integer (
            '1234' => '1234',
            $.headers['REF-ID'] => '9023470X945622',
            json:{} => json:{}
          ) => ERROR(Expected '9023470X945622' to match an integer number)
        ) => ERROR(Expected '9023470X945622' to match an integer number)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': '9023470X945622'},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1111".to_string(), "2222".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => ['1111', '2222']
          ) => BOOL(true),
          %match:integer (
            '1234' => '1234',
            $.headers['REF-ID'] => ['1111', '2222'],
            json:{} => json:{}
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': ['1111', '2222']},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1111".to_string(), "two".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => ['1111', 'two']
          ) => BOOL(true),
          %match:integer (
            '1234' => '1234',
            $.headers['REF-ID'] => ['1111', 'two'],
            json:{} => json:{}
          ) => ERROR(Expected 'two' to match an integer number)
        ) => ERROR(Expected 'two' to match an integer number)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': ['1111', 'two']},
        %join (
          'The following expected headers were missing: ',
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
}

#[test]
fn match_headers_with_min_type_matching_rules() {
  let matching_rules = matchingrules! {
    "header" => { "REF-ID" => [ MatchingRule::MinType(2) ] }
  };
  let expected_request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1234".to_string(), "4567".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
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

  let mut plan = ExecutionPlan::new("header-test");
  plan.add(setup_header_plan(&expected_request, &context.for_headers()).unwrap());

  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE']
          ),
          %match:equality (
            'test',
            $.headers['REF-CODE'],
            NULL
          )
        )
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID']
          ),
          %match:min-type (
            ['1234', '4567'],
            $.headers['REF-ID'],
            json:{"min":2}
          )
        )
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'],
        $.headers,
        %join (
          'The following expected headers were missing: ',
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

  let request = HttpRequest {
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1".to_string(), "1".to_string(), "1".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => ['1', '1', '1']
          ) => BOOL(true),
          %match:min-type (
            ['1234', '4567'] => ['1234', '4567'],
            $.headers['REF-ID'] => ['1', '1', '1'],
            json:{"min":2} => json:{"min":2}
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': ['1', '1', '1']},
        %join (
          'The following expected headers were missing: ',
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
    headers: Some(hashmap!{
      "REF-ID".to_string() => vec!["1".to_string()],
      "REF-CODE".to_string() => vec!["test".to_string()]
    }),
    .. HttpRequest::default()
  };
  let executed_plan = execute_request_plan(&plan, &request, &mut context).unwrap();
  pretty_assertions::assert_eq!(r#"(
  :header-test (
    :headers (
      :REF-CODE (
        %if (
          %check:exists (
            $.headers['REF-CODE'] => 'test'
          ) => BOOL(true),
          %match:equality (
            'test' => 'test',
            $.headers['REF-CODE'] => 'test',
            NULL => NULL
          ) => BOOL(true)
        ) => BOOL(true)
      ),
      :REF-ID (
        %if (
          %check:exists (
            $.headers['REF-ID'] => '1'
          ) => BOOL(true),
          %match:min-type (
            ['1234', '4567'] => ['1234', '4567'],
            $.headers['REF-ID'] => '1',
            json:{"min":2} => json:{"min":2}
          ) => ERROR(Expected [1] (size 1) to have minimum size of 2)
        ) => ERROR(Expected [1] (size 1) to have minimum size of 2)
      ),
      %expect:entries (
        ['REF-CODE', 'REF-ID'] => ['REF-CODE', 'REF-ID'],
        $.headers => {'REF-CODE': 'test', 'REF-ID': '1'},
        %join (
          'The following expected headers were missing: ',
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
}
