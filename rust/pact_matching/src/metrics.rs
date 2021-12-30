//! Metrics sent to GA.
//!
//! This module defines some events that can be used to capture usage metrics and send them
//! to a Google Analytics account.

use std::cell::RefCell;
use std::env::consts::{ARCH, OS};
use std::env::var;
use std::process::Command;
use std::str;
use std::sync::Mutex;

use anyhow::anyhow;
use lazy_static::lazy_static;
use log::{debug, warn};
use maplit::hashmap;
use reqwest::Client;
use uuid::Uuid;

static CIS: &'static [&str] = &[
  "CI",
  "CONTINUOUS_INTEGRATION",
  "BSTRUSE_BUILD_DIR",
  "APPVEYOR",
  "BUDDY_WORKSPACE_URL",
  "BUILDKITE",
  "CF_BUILD_URL",
  "CIRCLECI",
  "CODEBUILD_BUILD_ARN",
  "CONCOURSE_URL",
  "DRONE",
  "GITLAB_CI",
  "GO_SERVER_URL",
  "JENKINS_URL",
  "PROBO_ENVIRONMENT",
  "SEMAPHORE",
  "SHIPPABLE",
  "TDDIUM",
  "TEAMCITY_VERSION",
  "TF_BUILD",
  "TRAVIS",
  "WERCKER_ROOT"
];

/// Metric events to send
pub enum MetricEvent {
  /// Consumer test was run (number of interactions)
  ConsumerTestRun {
    /// Number of interactions in the test
    interactions: usize,
    /// Test framework used
    test_framework: String,
    /// Application name that executed the test
    app_name: String,
    /// Application version that executed the test
    app_version: String
  },

  /// Provider verification test ran (mode build tool or unit test etc.)
  ProviderVerificationRan {
    /// Number of verification tests run
    tests_run: usize,
    /// Test framework used
    test_framework: String,
    /// Application name that executed the test
    app_name: String,
    /// Application version that executed the test
    app_version: String
  }
}

impl MetricEvent {
  /// Application name for the event. For FFI calls, this will be the app doing the calling.
  pub(crate) fn app_name(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { app_name, .. } => app_name.as_str(),
      MetricEvent::ProviderVerificationRan { app_name, .. } => app_name.as_str()
    }
  }

  /// Application version for the event. For FFI calls, this will be the app doing the calling.
  pub(crate) fn app_version(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { app_version, .. } => app_version.as_str(),
      MetricEvent::ProviderVerificationRan { app_version, .. } => app_version.as_str()
    }
  }

  /// Test framework used (unit test framework or build tool)
  pub(crate) fn test_framework(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { test_framework, .. } => test_framework.as_str(),
      MetricEvent::ProviderVerificationRan { test_framework, .. } => test_framework.as_str()
    }
  }

  /// Event name
  pub(crate) fn name(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { .. } => "Pact consumer tests ran",
      MetricEvent::ProviderVerificationRan { .. } => "Pacts verified"
    }
  }

  /// Event category
  pub(crate) fn category(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { .. } => "ConsumerTest",
      MetricEvent::ProviderVerificationRan { .. } => "ProviderTest"
    }
  }

  /// Event action that occurred
  pub(crate) fn action(&self) -> &str {
    match self {
      MetricEvent::ConsumerTestRun { .. } => "Completed",
      MetricEvent::ProviderVerificationRan { .. } => "Completed"
    }
  }

  /// Value for the event
  pub(crate) fn value(&self) -> String {
    match self {
      MetricEvent::ConsumerTestRun { interactions, .. } => interactions.to_string(),
      MetricEvent::ProviderVerificationRan { tests_run, .. } => tests_run.to_string()
    }
  }
}

const GA_ACCOUNT: &str = "UA-117778936-1";
const GA_URL: &str = "https://www.google-analytics.com/collect";

lazy_static! {
  static ref WARNING_LOGGED: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
}

/// This sends anonymous metrics to a Google Analytics account. It is used to track usage of
/// Pact library and operating system versions. This can be disabled by setting the
/// `pact_do_not_track` environment variable to `true`.
///
/// This function needs to run in the context of a Tokio runtime.
pub fn send_metrics(event: MetricEvent) {
  let do_not_track = match var("pact_do_not_track") {
    Ok(val) => val == "true",
    Err(_) => false
  };

  if do_not_track {
    debug!("'pact_do_not_track' environment variable is set, will not send metrics");
  } else {
    match tokio::runtime::Handle::try_current() {
      Ok(handle) => {
        let mut guard = WARNING_LOGGED.lock().unwrap();
        let warning_logged = (*guard).get_mut();
        if *warning_logged == false {
          warn!(
            "\n\nPlease note:\n\
            Please note: we are tracking events anonymously to gather important usage statistics like Pact version \
            and operating system. To disable tracking, set the 'pact_do_not_track' system property or environment \
            variable to 'true'.\n\n"
          );
          *warning_logged = true;
        }

        handle.spawn(async move {
          let ci_context = if CIS.iter()
            .any(|n| var(n).map(|val| !val.is_empty()).unwrap_or(false)) {
            "CI"
          } else {
            "unknown"
          };
          let osarch = format!("{}-{}", OS, ARCH);
          let uid = hostname_hash();
          let value = event.value();

          let event_payload = hashmap!{
            "v" => "1",                                       // Version of the API
            "t" => "event",                                   // Hit type, Specifies the metric is for an event
            "tid" => GA_ACCOUNT,                              // Property ID
            "cid" => uid.as_str(),                            // Anonymous Client ID.
            "an" => event.app_name(),                         // App name.
            "aid" => event.app_name(),                        // App Id
            "av" => event.app_version(),                      // App version.
            "aip" => "true",                                  // Anonymise IP address
            "ds" => "client",                                 // Data source
            "cd2" => ci_context,                              // Custom Dimension 2: context
            "cd3" => osarch.as_str(),                         // Custom Dimension 3: osarch
            "cd6" => event.test_framework(),                  // Custom Dimension 6: test_framework
            "cd7" => env!("CARGO_PKG_VERSION"),               // Custom Dimension 7: platform_version
            "el" => event.name(),                             // Event
            "ec" => event.category(),                         // Category
            "ea" => event.action(),                           // Action
            "ev" => value.as_str()                            // Value
          };
          debug!("Sending event to GA - {:?}", event_payload);
          let result = Client::new().post(GA_URL)
            .form(&event_payload)
            .send()
            .await;
          if let Err(err) = result {
            debug!("Failed to post event - {}", err);
          }
        });
      },
      Err(err) => {
        debug!("Could not get the tokio runtime, will not send metrics - {}", err)
      }
    }
  }
}

/// Calculates a one-way hash of the hostname where the event occurred
fn hostname_hash() -> String {
  let host_name = if OS == "windows" {
    var("COMPUTERNAME")
  } else {
    var("HOSTNAME")
  }.or_else(|_| {
    exec_hostname_command()
  }).unwrap_or_else(|_| {
    Uuid::new_v4().to_string()
  });

  let digest = md5::compute(host_name.as_bytes());
  format!("{:x}", digest)
}

/// Execute the hostname command to get the hostname
fn exec_hostname_command() -> anyhow::Result<String> {
  match Command::new("hostname").output() {
    Ok(output) => if output.status.success() {
      Ok(str::from_utf8(&*output.stdout)?.to_string())
    } else {
      Err(anyhow!("Failed to invoke hostname command: status {}", output.status))
    }
    Err(err) => Err(anyhow!("Failed to invoke hostname command: {}", err))
  }
}