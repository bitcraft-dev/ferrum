// Type mapping — Ferrum Type → Rust type string.
//
// This module answers: "what Rust type should I write for a given
// Ferrum type in a specific position (variable, parameter, return)?"
//
// POSITIONS
//   Local variable  — simple type, value semantics
//   Function param  — may be reference depending on ownership
//   Return type     — never a reference in generated code
//   Struct field    — stored by value

use crate::ast::{DataType, Ownership, Type};
use crate::codegen::context::BoardProfile;

/// Rust type string for a Ferrum DataType in a local variable position.
pub fn rust_type_for_data(ty: &DataType) -> &'static str {
    match ty {
        DataType::Integer    => "i32",
        DataType::Decimal    => "f32",
        DataType::Percentage => "f32",   // stored as f32, constrained 0.0–100.0
        DataType::Boolean    => "bool",
        DataType::String     => "&'static str",
        DataType::Byte       => "u8",
    }
}

/// Rust type string for a Ferrum Type in a local variable position.
pub fn rust_type(ty: &Type) -> String {
    match ty {
        Type::Integer    => "i32".into(),
        Type::Decimal    => "f32".into(),
        Type::Percentage => "f32".into(),
        Type::Boolean    => "bool".into(),
        Type::String     => "&'static str".into(),
        Type::Byte       => "u8".into(),
        Type::PinState   => "bool".into(),   // HIGH=true, LOW=false
        Type::AnalogRaw  => "u16".into(),    // 0–1023 fits in u16
        Type::Array(elem, size) => format!("[{}; {}]", rust_type(elem), size),
        Type::Device(ident) => crate::codegen::name_mangler::to_pascal(ident),
        Type::Void       => "()".into(),
    }
}

/// Rust type string for a device parameter given an ownership keyword.
///
///   GIVE   → DeviceType             (pass by value / move)
///   LEND   → &DeviceType            (shared reference)
///   BORROW → &mut DeviceType        (mutable reference)
pub fn rust_device_param_type(device_type_name: &str, ownership: &Ownership) -> String {
    match ownership {
        Ownership::Give   => device_type_name.to_string(),
        Ownership::Lend   => format!("&{}", device_type_name),
        Ownership::Borrow => format!("&mut {}", device_type_name),
    }
}

/// Rust call-site expression for passing a device argument.
///
///   GIVE   → device_name            (move — no prefix)
///   LEND   → &device_name
///   BORROW → &mut device_name
pub fn rust_device_arg(device_name: &str, ownership: &Ownership) -> String {
    match ownership {
        Ownership::Give   => device_name.to_string(),
        Ownership::Lend   => format!("&{}", device_name),
        Ownership::Borrow => format!("&mut {}", device_name),
    }
}

/// Rust type for a hardware interface field in a device struct.
pub fn rust_interface_field_type(profile: &BoardProfile, iface: &crate::ast::InterfaceType) -> &'static str {
    match iface {
        crate::ast::InterfaceType::Output      => profile.output_pin_type,
        crate::ast::InterfaceType::Input       => profile.input_pin_type,
        crate::ast::InterfaceType::AnalogInput => profile.analog_pin_type,
        crate::ast::InterfaceType::Pwm         => profile.pwm_pin_type,
        crate::ast::InterfaceType::Display     => profile.display_type,
        crate::ast::InterfaceType::Pulse       => profile.input_pin_type,
    }
}

#[cfg(test)]
mod type_map_tests {
    use super::*;
    use crate::ast::Ownership;

    #[test]
    fn data_type_mapping() {
        assert_eq!(rust_type_for_data(&DataType::Integer),    "i32");
        assert_eq!(rust_type_for_data(&DataType::Boolean),    "bool");
        assert_eq!(rust_type_for_data(&DataType::Byte),       "u8");
        assert_eq!(rust_type_for_data(&DataType::Percentage), "f32");
    }

    #[test]
    fn ownership_to_reference() {
        assert_eq!(rust_device_param_type("Led", &Ownership::Give),   "Led");
        assert_eq!(rust_device_param_type("Led", &Ownership::Lend),   "&Led");
        assert_eq!(rust_device_param_type("Led", &Ownership::Borrow), "&mut Led");
    }

    #[test]
    fn device_arg_prefix() {
        assert_eq!(rust_device_arg("status_led", &Ownership::Give),   "status_led");
        assert_eq!(rust_device_arg("status_led", &Ownership::Lend),   "&status_led");
        assert_eq!(rust_device_arg("status_led", &Ownership::Borrow), "&mut status_led");
    }

    #[test]
    fn array_type_formatting() {
        assert_eq!(rust_type(&Type::Array(Box::new(Type::Integer), 5)), "[i32; 5]");
    }
}