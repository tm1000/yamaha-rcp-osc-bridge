use rosc::{OscMessage, OscType};
use yamaha_rcp_to_osc::{rcp_to_osc, rcp_to_osc_type, split_respecting_quotes, osc_to_rcp, osc_to_rcp_arg};

#[test]
fn test_rcp_to_osc_type() {
    // Test integer conversion
    let int_arg = "42".to_string();
    assert!(matches!(rcp_to_osc_type(&int_arg), OscType::Int(42)));

    // Test float conversion
    let float_arg = "3.14".to_string();
    assert!(matches!(rcp_to_osc_type(&float_arg), OscType::Float(f) if (f - 3.14).abs() < f32::EPSILON));

    // Test string conversion
    let string_arg = "test".to_string();
    assert!(matches!(rcp_to_osc_type(&string_arg), OscType::String(s) if s == "test"));
}

#[test]
fn test_split_respecting_quotes() {
    // Test basic splitting
    let basic = "command arg1 arg2";
    assert_eq!(split_respecting_quotes(basic), vec!["command", "arg1", "arg2"]);

    // Test quoted strings
    let quoted = r#"command "arg with spaces" arg2"#;
    assert_eq!(split_respecting_quotes(quoted), vec!["command", "\"arg with spaces\"", "arg2"]);

    // Test empty string
    let empty = "";
    assert!(split_respecting_quotes(empty).is_empty());

    // Test string with multiple spaces
    let multi_space = "command   arg1    arg2";
    assert_eq!(split_respecting_quotes(multi_space), vec!["command", "arg1", "arg2"]);
}

#[test]
fn test_osc_to_rcp_arg() {
    // Test integer conversion
    assert_eq!(osc_to_rcp_arg(&OscType::Int(42)).unwrap(), "42");

    // Test float conversion
    assert_eq!(osc_to_rcp_arg(&OscType::Float(3.14)).unwrap(), "3.14");

    // Test string conversion
    assert_eq!(osc_to_rcp_arg(&OscType::String("test".to_string())).unwrap(), "\"test\"");

    // Test unsupported type
    assert!(osc_to_rcp_arg(&OscType::Nil).is_err());
}

#[test]
fn test_rcp_to_osc() {
    // Test NOTIFY message
    let notify_msg = "NOTIFY scene current 1".to_string();
    let osc_msg = rcp_to_osc(notify_msg).unwrap();
    assert_eq!(osc_msg.addr, "/scene/current");
    assert_eq!(osc_msg.args.len(), 1);
    assert!(matches!(&osc_msg.args[0], OscType::Int(1)));

    // Test OK message
    let ok_msg = "OK scene current 2".to_string();
    let osc_msg = rcp_to_osc(ok_msg).unwrap();
    assert_eq!(osc_msg.addr, "/scene/current");
    assert_eq!(osc_msg.args.len(), 1);
    assert!(matches!(&osc_msg.args[0], OscType::Int(2)));

    // Test ERROR message
    let error_msg = "ERROR some error message".to_string();
    let osc_msg = rcp_to_osc(error_msg).unwrap();
    assert_eq!(osc_msg.addr, "/error");
    assert_eq!(osc_msg.args.len(), 3);

    // Test invalid message
    let invalid_msg = "INVALID message".to_string();
    assert!(rcp_to_osc(invalid_msg).is_err());
}

#[test]
fn test_osc_to_rcp() {
    // Test basic message
    let osc_msg = OscMessage {
        addr: "/scene/current".to_string(),
        args: vec![OscType::Int(1)],
    };
    assert_eq!(osc_to_rcp(&osc_msg).unwrap(), "scene current 1");

    // Test message with multiple arguments
    let osc_msg = OscMessage {
        addr: "/scene/name".to_string(),
        args: vec![
            OscType::Int(1),
            OscType::String("Test Scene".to_string()),
        ],
    };
    assert_eq!(osc_to_rcp(&osc_msg).unwrap(), r#"scene name 1 "Test Scene""#);

    // Test invalid address
    let invalid_msg = OscMessage {
        addr: "".to_string(),
        args: vec![],
    };
    assert!(osc_to_rcp(&invalid_msg).is_err());
}

#[test]
fn test_bidirectional_conversion() {
    // Test RCP -> OSC -> RCP conversion
    let original_rcp = "NOTIFY scene current 1".to_string();
    let osc = rcp_to_osc(original_rcp.clone()).unwrap();
    let rcp = osc_to_rcp(&osc).unwrap();
    assert_eq!(rcp, "scene current 1");

    // Test with quoted strings
    let original_rcp = "NOTIFY scene name 1 \"Test Scene\"".to_string();
    let osc = rcp_to_osc(original_rcp.clone()).unwrap();
    let rcp = osc_to_rcp(&osc).unwrap();
    assert_eq!(rcp, "scene name 1 \"Test Scene\"");
}
