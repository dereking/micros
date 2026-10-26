use micro_store::{AppMeta, ConfigDoc, KvValue, StoreError};

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

#[test]
fn config_doc_reads_and_writes_flat_scalars() {
    let mut document =
        ConfigDoc::parse(r#"{"maimemoToken":"abc","goal":5,"on":true}"#).expect("parse config");
    assert_eq!(
        document.get("maimemoToken"),
        Some(KvValue::String("abc".into()))
    );
    assert_eq!(document.get("goal"), Some(KvValue::Number(5.0)));
    assert_eq!(document.get("on"), Some(KvValue::Bool(true)));
    assert_eq!(document.get("missing"), None);

    document
        .set("goal", &KvValue::Number(8.0))
        .expect("set goal");
    document
        .set("note", &KvValue::String("hi".into()))
        .expect("set note");
    assert_eq!(document.get("goal"), Some(KvValue::Number(8.0)));
    document.remove("on");
    assert_eq!(document.get("on"), None);

    let round_tripped = ConfigDoc::parse(&document.to_json()).expect("round trip");
    assert_eq!(round_tripped, document);
}

#[test]
fn config_doc_accepts_empty_input_and_rejects_corruption() {
    assert!(ConfigDoc::parse("").expect("empty").is_empty());
    assert!(ConfigDoc::parse("   \n").expect("blank").is_empty());
    assert!(matches!(
        ConfigDoc::parse("not json"),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn config_doc_skips_nested_values_instead_of_failing() {
    let document = ConfigDoc::parse(r#"{"meta":{"version":2},"goal":5}"#).expect("parse");
    assert_eq!(document.get("goal"), Some(KvValue::Number(5.0)));
    assert_eq!(document.get("meta"), None);
}

#[test]
fn config_doc_rejects_invalid_keys_and_non_finite_numbers() {
    let mut document = ConfigDoc::default();
    assert!(matches!(
        document.set("", &KvValue::Number(1.0)),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(
        document.set("bad\0key", &KvValue::Number(1.0)),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(
        document.set("nan", &KvValue::Number(f64::NAN)),
        Err(StoreError::Unsupported(_))
    ));
}
