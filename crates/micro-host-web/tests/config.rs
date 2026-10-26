//! Off-wasm checks for the Web host's `config` routing. The real browser
//! persistence path is covered by the Playwright suite; here the backing store
//! is the in-memory `ConfigDoc`, so the scalar/namespace behavior is identical.

use micro_host_web::host::WebHost;
use micro_ir::{HostCallKind, HostRequest, ScalarType};
use micro_vm::{HostAccess, Value};

fn sync(kind: HostCallKind, arg_kinds: Vec<ScalarType>) -> HostRequest {
    HostRequest::sync(kind, arg_kinds, None)
}

#[test]
fn web_config_round_trips_and_is_namespaced_by_app() {
    let mut momo = WebHost::new_for_app("momo");
    momo.call(
        &sync(
            HostCallKind::ConfigSetText,
            vec![ScalarType::String, ScalarType::String],
        ),
        &[
            Value::String("maimemoToken".into()),
            Value::String("secret".into()),
        ],
    )
    .expect("set token");
    momo.call(
        &sync(
            HostCallKind::ConfigSetNumber,
            vec![ScalarType::String, ScalarType::Number],
        ),
        &[Value::String("goal".into()), Value::Number(5.0)],
    )
    .expect("set goal");

    let token = momo
        .call(
            &sync(HostCallKind::ConfigGetText, vec![ScalarType::String]),
            &[Value::String("maimemoToken".into())],
        )
        .expect("get token");
    assert_eq!(token, Some(Value::String("secret".into())));
    let goal = momo
        .call(
            &sync(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
            &[Value::String("goal".into())],
        )
        .expect("get goal");
    assert_eq!(goal, Some(Value::Number(5.0)));

    let mut counter = WebHost::new_for_app("counter");
    let other = counter
        .call(
            &sync(HostCallKind::ConfigGetText, vec![ScalarType::String]),
            &[Value::String("maimemoToken".into())],
        )
        .expect("cross-app read");
    assert_eq!(other, Some(Value::String(String::new())));

    momo.call(
        &sync(HostCallKind::ConfigRemove, vec![ScalarType::String]),
        &[Value::String("goal".into())],
    )
    .expect("remove");
    let removed = momo
        .call(
            &sync(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
            &[Value::String("goal".into())],
        )
        .expect("get removed");
    assert_eq!(removed, Some(Value::Number(0.0)));
}

#[test]
fn web_config_reads_legacy_kv_values_and_promotes_them() {
    let mut host = WebHost::new_for_app("momo");
    // Pre-v22 momo kept the token and goal in the KV namespace.
    host.call(
        &sync(
            HostCallKind::KvSetText,
            vec![ScalarType::String, ScalarType::String],
        ),
        &[
            Value::String("maimemoToken".into()),
            Value::String("legacy".into()),
        ],
    )
    .expect("seed token");
    host.call(
        &sync(
            HostCallKind::KvSetNumber,
            vec![ScalarType::String, ScalarType::Number],
        ),
        &[Value::String("goal".into()), Value::Number(8.0)],
    )
    .expect("seed goal");

    let token = host
        .call(
            &sync(HostCallKind::ConfigGetText, vec![ScalarType::String]),
            &[Value::String("maimemoToken".into())],
        )
        .expect("read legacy token");
    assert_eq!(token, Some(Value::String("legacy".into())));
    let goal = host
        .call(
            &sync(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
            &[Value::String("goal".into())],
        )
        .expect("read legacy goal");
    assert_eq!(goal, Some(Value::Number(8.0)));

    host.call(
        &sync(
            HostCallKind::ConfigSetText,
            vec![ScalarType::String, ScalarType::String],
        ),
        &[
            Value::String("maimemoToken".into()),
            Value::String("fresh".into()),
        ],
    )
    .expect("write token");
    let token = host
        .call(
            &sync(HostCallKind::ConfigGetText, vec![ScalarType::String]),
            &[Value::String("maimemoToken".into())],
        )
        .expect("read fresh token");
    assert_eq!(token, Some(Value::String("fresh".into())));
    let legacy = host
        .call(
            &sync(HostCallKind::KvGetText, vec![ScalarType::String]),
            &[Value::String("maimemoToken".into())],
        )
        .expect("kv token");
    assert_eq!(
        legacy,
        Some(Value::String(String::new())),
        "the promoted key leaves the KV namespace"
    );
    let goal = host
        .call(
            &sync(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
            &[Value::String("goal".into())],
        )
        .expect("read legacy goal");
    assert_eq!(goal, Some(Value::Number(8.0)));
}
