use micro_store::{AppMeta, KvValue, StoreError};

#[test]
fn app_meta_round_trips_through_json() {
    let meta = AppMeta {
        id: "counter".to_owned(),
        name: "Counter".to_owned(),
        version: 3,
    };
    let encoded = serde_json::to_vec(&meta).expect("serialize");
    let decoded: AppMeta = serde_json::from_slice(&encoded).expect("deserialize");
    assert_eq!(decoded, meta);
}

#[test]
fn store_error_is_a_std_error() {
    let error: Box<dyn std::error::Error> = Box::new(StoreError::NotFound);
    assert!(!error.to_string().is_empty());
}

#[test]
fn kv_value_round_trips_all_scalar_types() {
    let cases = [
        (KvValue::Number(42.0), serde_json::json!(42.0)),
        (KvValue::String("hi".to_owned()), serde_json::json!("hi")),
        (KvValue::Bool(true), serde_json::json!(true)),
        (KvValue::Null, serde_json::Value::Null),
    ];
    for (value, json) in cases {
        assert_eq!(value.to_json(), json, "to_json for {value:?}");
        assert_eq!(KvValue::from_json(json).expect("from_json"), value);
    }
}
