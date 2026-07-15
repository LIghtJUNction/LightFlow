#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_input_metadata {
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident $widget:ident $artifact:ident $model:ident];) => { $builder };
    ($builder:ident, $name:literal; [$($seen:ident)*]; , $($rest:tt)*) => {
        $crate::__lightflow_input_metadata!($builder, $name; [$($seen)*]; $($rest)*)
    };

    ($builder:ident, $name:literal; [no $r:ident $default:ident $range:ident $choices:ident $widget:ident $artifact:ident $model:ident]; description: $value:literal, $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_description($name, $value);
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [yes $r $default $range $choices $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [yes $($seen:ident)*]; description: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: description") };

    ($builder:ident, $name:literal; [$d:ident no $default:ident $range:ident $choices:ident $widget:ident $artifact:ident $model:ident]; required: true, $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_required($name, true);
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d yes $default $range $choices $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [$d:ident no $default:ident $range:ident $choices:ident $widget:ident $artifact:ident $model:ident]; required: false, $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_required($name, false);
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d yes $default $range $choices $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [$d:ident yes $($seen:ident)*]; required: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: required") };
    ($builder:ident, $name:literal; [$d:ident no $($seen:ident)*]; required: $value:expr, $($rest:tt)*) => { compile_error!("input required must be the literal true or false") };

    ($builder:ident, $name:literal; [$d:ident $r:ident no $range:ident $choices:ident $widget:ident $artifact:ident $model:ident]; default: - $value:literal, $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_default($name, $crate::__lightflow_json!(-$value));
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r yes $range $choices $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [$d:ident $r:ident no $range:ident $choices:ident $widget:ident $artifact:ident $model:ident]; default: $value:tt, $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_default($name, $crate::__lightflow_json!($value));
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r yes $range $choices $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [$d:ident $r:ident yes $($seen:ident)*]; default: {$($value:tt)*}, $($rest:tt)*) => { compile_error!("duplicate input metadata key: default") };
    ($builder:ident, $name:literal; [$d:ident $r:ident yes $($seen:ident)*]; default: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: default") };
    ($builder:ident, $name:literal; [$d:ident $r:ident no $($seen:ident)*]; default: $value:expr, $($rest:tt)*) => { compile_error!("input default must be a JSON literal, array, or object") };

    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident no $choices:ident $widget:ident $artifact:ident $model:ident]; range: [$($value:tt)*], $($rest:tt)*) => {
        $crate::__lightflow_input_range!($builder, $name; [$d $r $default $choices $widget $artifact $model]; [$($value)*]; $($rest)*)
    };
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident yes $($seen:ident)*]; range: [$($value:tt)*], $($rest:tt)*) => { compile_error!("duplicate input metadata key: range") };
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident no $($seen:ident)*]; range: $value:expr, $($rest:tt)*) => { compile_error!("input range must contain exactly three numeric literals") };

    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident no $widget:ident $artifact:ident $model:ident]; choices: [$($value:tt)*], $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input_choices($name, $crate::__lightflow_json!([$($value)*]));
        $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r $default $range yes $widget $artifact $model]; $($rest)*)
    }};
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident yes $($seen:ident)*]; choices: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: choices") };
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident no $($seen:ident)*]; choices: $value:expr, $($rest:tt)*) => { compile_error!("input choices must be a JSON-compatible array") };

    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident no $artifact:ident $model:ident]; widget: $value:literal, $($rest:tt)*) => {{ let __lightflow_builder = $builder.input_widget($name, $value); $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r $default $range $choices yes $artifact $model]; $($rest)*) }};
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident yes $($seen:ident)*]; widget: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: widget") };
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident $widget:ident no $model:ident]; artifact: $value:literal, $($rest:tt)*) => {{ let __lightflow_builder = $builder.input_artifact_kind($name, $value); $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r $default $range $choices $widget yes $model]; $($rest)*) }};
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident $widget:ident yes $($seen:ident)*]; artifact: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: artifact") };
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident $widget:ident $artifact:ident no]; model: $value:literal, $($rest:tt)*) => {{ let __lightflow_builder = $builder.input_model_requirement($name, $value); $crate::__lightflow_input_metadata!(__lightflow_builder, $name; [$d $r $default $range $choices $widget $artifact yes]; $($rest)*) }};
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $range:ident $choices:ident $widget:ident $artifact:ident yes]; model: $value:expr, $($rest:tt)*) => { compile_error!("duplicate input metadata key: model") };
    ($builder:ident, $name:literal; [$($seen:ident)*]; $unknown:ident : $($rest:tt)*) => { compile_error!(concat!("unsupported input metadata key: ", stringify!($unknown))) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_input_range {
    ($builder:ident, $name:literal; [$($state:ident)*]; [$min:literal, $max:literal, $step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, $min as f64, $max as f64, $step as f64); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [-$min:literal, $max:literal, $step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, -($min as f64), $max as f64, $step as f64); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [$min:literal, -$max:literal, $step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, $min as f64, -($max as f64), $step as f64); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [$min:literal, $max:literal, -$step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, $min as f64, $max as f64, -($step as f64)); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [-$min:literal, -$max:literal, $step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, -($min as f64), -($max as f64), $step as f64); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [-$min:literal, $max:literal, -$step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, -($min as f64), $max as f64, -($step as f64)); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [$min:literal, -$max:literal, -$step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, $min as f64, -($max as f64), -($step as f64)); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [-$min:literal, -$max:literal, -$step:literal]; $($rest:tt)*) => {{ let __b = $builder.input_range($name, -($min as f64), -($max as f64), -($step as f64)); $crate::__lightflow_range_done!(__b, $name; [$($state)*]; $($rest)*) }};
    ($builder:ident, $name:literal; [$($state:ident)*]; [$($invalid:tt)*]; $($rest:tt)*) => { compile_error!("input range must contain exactly three numeric literals") };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_range_done {
    ($builder:ident, $name:literal; [$d:ident $r:ident $default:ident $choices:ident $widget:ident $artifact:ident $model:ident]; $($rest:tt)*) => {
        $crate::__lightflow_input_metadata!($builder, $name; [$d $r $default yes $choices $widget $artifact $model]; $($rest)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_output_metadata {
    ($builder:ident, $name:literal; [$d:ident $artifact:ident $model:ident];) => { $builder };
    ($builder:ident, $name:literal; [$($seen:ident)*]; , $($rest:tt)*) => { $crate::__lightflow_output_metadata!($builder, $name; [$($seen)*]; $($rest)*) };
    ($builder:ident, $name:literal; [no $artifact:ident $model:ident]; description: $value:literal, $($rest:tt)*) => {{ let __b = $builder.output_description($name, $value); $crate::__lightflow_output_metadata!(__b, $name; [yes $artifact $model]; $($rest)*) }};
    ($builder:ident, $name:literal; [yes $($seen:ident)*]; description: $value:expr, $($rest:tt)*) => { compile_error!("duplicate output metadata key: description") };
    ($builder:ident, $name:literal; [$d:ident no $model:ident]; artifact: $value:literal, $($rest:tt)*) => {{ let __b = $builder.output_artifact_kind($name, $value); $crate::__lightflow_output_metadata!(__b, $name; [$d yes $model]; $($rest)*) }};
    ($builder:ident, $name:literal; [$d:ident yes $model:ident]; artifact: $value:expr, $($rest:tt)*) => { compile_error!("duplicate output metadata key: artifact") };
    ($builder:ident, $name:literal; [$d:ident $artifact:ident no]; model: $value:literal, $($rest:tt)*) => {{ let __b = $builder.output_model_requirement($name, $value); $crate::__lightflow_output_metadata!(__b, $name; [$d $artifact yes]; $($rest)*) }};
    ($builder:ident, $name:literal; [$d:ident $artifact:ident yes]; model: $value:expr, $($rest:tt)*) => { compile_error!("duplicate output metadata key: model") };
    ($builder:ident, $name:literal; [$($seen:ident)*]; $unknown:ident : $($rest:tt)*) => { compile_error!(concat!("unsupported output metadata key: ", stringify!($unknown))) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_json {
    (null) => { $crate::serde_json::Value::Null };
    (true) => { $crate::serde_json::Value::Bool(true) };
    (false) => { $crate::serde_json::Value::Bool(false) };
    (- $value:literal) => {{
        const _: () = $crate::workflow::assert_workflow_json_literal(concat!("-", stringify!($value)));
        $crate::workflow::parse_workflow_json_literal(concat!("-", stringify!($value)))
    }};
    ($value:literal) => {{
        const _: () = $crate::workflow::assert_workflow_json_literal(stringify!($value));
        $crate::workflow::parse_workflow_json_literal(stringify!($value))
    }};
    ([$($values:tt)*]) => {
        $crate::serde_json::Value::Array($crate::__lightflow_json_array!($($values)* ,))
    };
    ({$($members:tt)*}) => {
        $crate::serde_json::Value::Object($crate::__lightflow_json_object!($($members)* ,))
    };
    ($($invalid:tt)+) => { compile_error!("value must use strict JSON literal syntax") };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_json_array {
    () => { ::std::vec::Vec::new() };
    (,) => { ::std::vec::Vec::new() };
    (, $($rest:tt)*) => { $crate::__lightflow_json_array!($($rest)*) };
    (- $value:literal, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!(-$value)];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    (null, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!(null)];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    (true, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!(true)];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    (false, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!(false)];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    ($value:literal, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!($value)];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    ([$($value:tt)*], $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!([$($value)*])];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    ({$($value:tt)*}, $($rest:tt)*) => {{
        let mut __values = ::std::vec![$crate::__lightflow_json!({$($value)*})];
        __values.extend($crate::__lightflow_json_array!($($rest)*));
        __values
    }};
    ($($invalid:tt)+) => { compile_error!("array items must use strict JSON literal syntax") };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_json_object {
    () => { $crate::serde_json::Map::new() };
    (,) => { $crate::serde_json::Map::new() };
    (, $($rest:tt)*) => { $crate::__lightflow_json_object!($($rest)*) };
    ($key:literal : - $value:literal, $($rest:tt)*) => {{
        let mut __object = $crate::serde_json::Map::new();
        __object.insert($crate::__lightflow_json_key!($key), $crate::__lightflow_json!(-$value));
        __object.extend($crate::__lightflow_json_object!($($rest)*));
        __object
    }};
    ($key:literal : $value:tt, $($rest:tt)*) => {{
        let mut __object = $crate::serde_json::Map::new();
        __object.insert($crate::__lightflow_json_key!($key), $crate::__lightflow_json!($value));
        __object.extend($crate::__lightflow_json_object!($($rest)*));
        __object
    }};
    ($($invalid:tt)+) => { compile_error!("object members require string literal keys and strict JSON values") };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lightflow_json_key {
    ($key:literal) => {{
        const _: () = $crate::workflow::assert_workflow_json_string_literal(stringify!($key));
        $crate::workflow::parse_workflow_json_string_literal(stringify!($key))
    }};
}
