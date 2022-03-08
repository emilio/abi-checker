use super::super::*;
use super::*;

pub static RUST_TEST_PREFIX: &str = include_str!("../../harness/rust_test_prefix.rs");

pub struct RustAbi;

impl Abi for RustAbi {
    fn name(&self) -> &'static str {
        "rust"
    }
    fn src_ext(&self) -> &'static str {
        "rs"
    }
    fn supports_convention(&self, convention: CallingConvention) -> bool {
        // NOTE: Rustc spits out:
        //
        // Rust, C, C-unwind, cdecl, stdcall, stdcall-unwind, fastcall,
        // vectorcall, thiscall, thiscall-unwind, aapcs, win64, sysv64,
        // ptx-kernel, msp430-interrupt, x86-interrupt, amdgpu-kernel,
        // efiapi, avr-interrupt, avr-non-blocking-interrupt, C-cmse-nonsecure-call,
        // wasm, system, system-unwind, rust-intrinsic, rust-call,
        // platform-intrinsic, unadjusted
        match convention {
            CallingConvention::All => unreachable!(),
            CallingConvention::Handwritten => true,
            CallingConvention::C => true,
            CallingConvention::System => true,
            CallingConvention::Win64 => true,
            CallingConvention::Sysv64 => true,
            CallingConvention::Aapcs => true,
            CallingConvention::Stdcall => true,
            CallingConvention::Fastcall => true,
            CallingConvention::Vectorcall => false, // experimental
        }
    }

    fn generate_caller(
        &self,
        f: &mut dyn Write,
        test: &Test,
        convention: CallingConvention,
    ) -> Result<(), BuildError> {
        write_rust_prefix(f, test)?;
        let convention_decl = convention.rust_convention_decl();

        // Generate the extern block
        writeln!(f, "extern \"{convention_decl}\" {{",)?;
        for function in &test.funcs {
            write!(f, "  ")?;
            write_rust_signature(f, function)?;
            writeln!(f, ";")?;
        }
        writeln!(f, "}}")?;
        writeln!(f)?;

        // Now generate the body
        writeln!(f, "#[no_mangle] pub extern \"C\" fn do_test() {{")?;

        for function in &test.funcs {
            if !function.has_convention(convention) {
                continue;
            }
            writeln!(f, "   unsafe {{")?;

            // Inputs
            for (idx, input) in function.inputs.iter().enumerate() {
                writeln!(
                    f,
                    "        {} = {};",
                    input.rust_var_decl(ARG_NAMES[idx])?,
                    input.rust_val()?
                )?;
            }
            writeln!(f)?;
            for (idx, input) in function.inputs.iter().enumerate() {
                writeln!(
                    f,
                    "{}",
                    input.rust_write_val("CALLER_INPUTS", ARG_NAMES[idx], true)?
                )?;
            }
            writeln!(f)?;

            // Outputs
            write!(f, "        ")?;
            let pass_out = if let Some(output) = &function.output {
                if let Some(decl) = output.rust_out_param_var(OUTPUT_NAME)? {
                    writeln!(f, "        {}", decl)?;
                    true
                } else {
                    write!(f, "        {} = ", output.rust_var_decl(OUTPUT_NAME)?)?;
                    false
                }
            } else {
                false
            };

            // Do the call
            write!(f, "{}(", function.name)?;
            for (idx, input) in function.inputs.iter().enumerate() {
                write!(f, "{}, ", input.rust_arg_pass(ARG_NAMES[idx])?)?;
            }
            if pass_out {
                writeln!(f, "&mut {OUTPUT_NAME}")?;
            }
            writeln!(f, ");")?;
            writeln!(f)?;

            // Report the output
            if let Some(output) = &function.output {
                writeln!(
                    f,
                    "{}",
                    output.rust_write_val("CALLER_OUTPUTS", OUTPUT_NAME, true)?
                )?;
            }

            // Finished
            writeln!(
                f,
                "        FINISHED_FUNC.unwrap()(CALLER_INPUTS, CALLER_OUTPUTS);"
            )?;
            writeln!(f, "   }}")?;
        }

        writeln!(f, "}}")?;

        Ok(())
    }
    fn generate_callee(
        &self,
        f: &mut dyn Write,
        test: &Test,
        convention: CallingConvention,
    ) -> Result<(), BuildError> {
        write_rust_prefix(f, test)?;
        let convention_decl = convention.rust_convention_decl();
        for function in &test.funcs {
            if !function.has_convention(convention) {
                continue;
            }
            // Write the signature
            writeln!(f, "#[no_mangle]")?;
            write!(f, "pub unsafe extern \"{convention_decl}\" ")?;
            write_rust_signature(f, function)?;
            writeln!(f, " {{")?;

            // Now the body

            // Report Inputs
            for (idx, input) in function.inputs.iter().enumerate() {
                writeln!(
                    f,
                    "{}",
                    input.rust_write_val("CALLEE_INPUTS", ARG_NAMES[idx], false)?
                )?;
            }
            writeln!(f)?;

            // Report outputs and return
            if let Some(output) = &function.output {
                let decl = output.rust_var_decl(OUTPUT_NAME)?;
                let val = output.rust_val()?;
                writeln!(f, "        {decl} = {val};")?;
                writeln!(
                    f,
                    "{}",
                    output.rust_write_val("CALLEE_OUTPUTS", OUTPUT_NAME, true)?
                )?;
                writeln!(
                    f,
                    "        FINISHED_FUNC.unwrap()(CALLEE_INPUTS, CALLEE_OUTPUTS);"
                )?;
                writeln!(
                    f,
                    "        {}",
                    output.rust_var_return(OUTPUT_NAME, OUT_PARAM_NAME)?
                )?;
            } else {
                writeln!(
                    f,
                    "        FINISHED_FUNC.unwrap()(CALLEE_INPUTS, CALLEE_OUTPUTS);"
                )?;
            }
            writeln!(f, "}}")?;
        }

        Ok(())
    }

    fn compile_callee(&self, src_path: &Path, lib_name: &str) -> Result<String, BuildError> {
        let out = Command::new("rustc")
            .arg("--crate-type")
            .arg("staticlib")
            .arg("--out-dir")
            .arg("target/temp/")
            .arg(src_path)
            .output()?;

        if !out.status.success() {
            Err(BuildError::RustCompile(out))
        } else {
            Ok(String::from(lib_name))
        }
    }
    fn compile_caller(&self, src_path: &Path, lib_name: &str) -> Result<String, BuildError> {
        // Currently no need to be different
        self.compile_callee(src_path, lib_name)
    }
}

/// Every test should start by loading in the harness' "header"
/// and forward-declaring any structs that will be used.
fn write_rust_prefix(f: &mut dyn Write, test: &Test) -> Result<(), BuildError> {
    // Load test harness "headers"
    write!(f, "{}", RUST_TEST_PREFIX)?;

    // Forward-decl struct types
    let mut forward_decls = std::collections::HashMap::<String, String>::new();
    for function in &test.funcs {
        for val in function.inputs.iter().chain(function.output.as_ref()) {
            for (name, decl) in val.rust_forward_decl()? {
                match forward_decls.entry(name) {
                    std::collections::hash_map::Entry::Occupied(entry) => {
                        if entry.get() != &decl {
                            return Err(BuildError::InconsistentStructDefinition {
                                name: entry.key().clone(),
                                old_decl: entry.remove(),
                                new_decl: decl,
                            });
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        writeln!(f, "{decl}")?;
                        entry.insert(decl);
                    }
                }
            }
        }
    }

    Ok(())
}

fn write_rust_signature(f: &mut dyn Write, function: &Func) -> Result<(), BuildError> {
    write!(f, "fn {}(", function.name)?;
    for (idx, input) in function.inputs.iter().enumerate() {
        write!(f, "{}, ", input.rust_arg_decl(ARG_NAMES[idx])?)?;
    }
    if let Some(output) = &function.output {
        if let Some(out_param) = output.rust_out_param(OUT_PARAM_NAME)? {
            write!(f, "{}", out_param)?;
            write!(f, ")")?;
        } else {
            write!(f, ")")?;
            let ty = output.rust_arg_type()?;
            write!(f, " -> {ty}")?;
        }
    } else {
        write!(f, ")")?;
    }
    Ok(())
}

// FIXME: it would be nice if more of this stuff used `write!` instead of `fmt!`
// but it's a bit of a pain in the ass to architect some of these operations that way.
impl Val {
    /// If this value defines a nominal type, this will spit out:
    ///
    /// * The type name
    /// * The forward-declaration of that type
    ///
    /// To catch buggy test definitions, you should validate that all
    /// structs that claim a particular name have the same declaration.
    /// This is done in write_rust_prefix.
    fn rust_forward_decl(&self) -> Result<Vec<(String, String)>, GenerateError> {
        use Val::*;
        match self {
            Struct(name, fields) => {
                let mut results = vec![];
                for field in fields.iter() {
                    results.extend(field.rust_forward_decl()?);
                }
                let mut output = String::new();
                let ref_name = format!("{name}");
                output.push_str("\n#[repr(C)]\n");
                output.push_str(&format!("pub struct {name} {{\n"));
                for (idx, field) in fields.iter().enumerate() {
                    let line =
                        format!("    {}: {},\n", FIELD_NAMES[idx], field.rust_nested_type()?);
                    output.push_str(&line);
                }
                output.push_str("}");
                results.push((ref_name, output));
                Ok(results)
            }
            Array(vals) => vals[0].rust_forward_decl(),
            Ref(x) => x.rust_forward_decl(),
            _ => Ok(vec![]),
        }
    }

    /// The decl to use for a local var (reference-ness stripped)
    fn rust_var_decl(&self, var_name: &str) -> Result<String, GenerateError> {
        if let Val::Ref(x) = self {
            Ok(x.rust_var_decl(var_name)?)
        } else {
            Ok(format!("let {var_name}: {}", self.rust_arg_type()?))
        }
    }

    /// The decl to use for a function arg (apply referenceness)
    fn rust_arg_decl(&self, arg_name: &str) -> Result<String, GenerateError> {
        if let Val::Ref(x) = self {
            Ok(format!("{arg_name}: &{}", x.rust_arg_type()?))
        } else {
            Ok(format!("{arg_name}: {}", self.rust_arg_type()?))
        }
    }

    /// If the return type needs to be an out_param, this returns it
    fn rust_out_param(&self, out_param_name: &str) -> Result<Option<String>, GenerateError> {
        if let Val::Ref(x) = self {
            Ok(Some(format!(
                "{out_param_name}: &mut {}",
                x.rust_arg_type()?
            )))
        } else {
            Ok(None)
        }
    }

    /// If the return type needs to be an out_param, this returns it
    fn rust_out_param_var(&self, output_name: &str) -> Result<Option<String>, GenerateError> {
        if let Val::Ref(x) = self {
            Ok(Some(format!(
                "let mut {output_name}: {} = {};",
                x.rust_arg_type()?,
                x.rust_default_val()?
            )))
        } else {
            Ok(None)
        }
    }

    /// How to pass an argument
    fn rust_arg_pass(&self, arg_name: &str) -> Result<String, GenerateError> {
        if let Val::Ref(_) = self {
            Ok(format!("&{arg_name}"))
        } else {
            Ok(format!("{arg_name}"))
        }
    }

    /// How to return a value
    fn rust_var_return(
        &self,
        var_name: &str,
        out_param_name: &str,
    ) -> Result<String, GenerateError> {
        if let Val::Ref(_) = self {
            Ok(format!("*{out_param_name} = {var_name};"))
        } else {
            Ok(format!("return {var_name};"))
        }
    }

    /// The type name to use for this value when it is stored in args/vars.
    fn rust_arg_type(&self) -> Result<String, GenerateError> {
        use IntVal::*;
        use Val::*;
        let val = match self {
            Ref(x) => format!("*mut {}", x.rust_arg_type()?),
            Ptr(_) => format!("*mut ()"),
            Bool(_) => format!("bool"),
            Array(vals) => format!(
                "[{}; {}]",
                vals.get(0).unwrap_or(&Val::Ptr(0)).rust_arg_type()?,
                vals.len()
            ),
            Struct(name, _) => format!("{name}"),
            Float(FloatVal::c_double(_)) => format!("f64"),
            Float(FloatVal::c_float(_)) => format!("f32"),
            Int(int_val) => match int_val {
                c__int128(_) => format!("i128"),
                c_int64_t(_) => format!("i64"),
                c_int32_t(_) => format!("i32"),
                c_int16_t(_) => format!("i16"),
                c_int8_t(_) => format!("i8"),
                c__uint128(_) => format!("u128"),
                c_uint64_t(_) => format!("u64"),
                c_uint32_t(_) => format!("u32"),
                c_uint16_t(_) => format!("u16"),
                c_uint8_t(_) => format!("u8"),
            },
        };
        Ok(val)
    }

    /// The type name to use for this value when it is stored in composite.
    ///
    /// This is separated out in case there's a type that needs different
    /// handling in this context to conform to a layout (i.e. how C arrays
    /// decay into pointers when used in function args).
    fn rust_nested_type(&self) -> Result<String, GenerateError> {
        self.rust_arg_type()
    }

    /// An expression that generates this value.
    fn rust_val(&self) -> Result<String, GenerateError> {
        use IntVal::*;
        use Val::*;
        let val = match self {
            Ref(x) => x.rust_val()?,
            Ptr(addr) => format!("{addr} as *mut ()"),
            Bool(val) => format!("{val}"),
            Array(vals) => {
                let mut output = String::new();
                output.push_str(&format!("[",));
                for val in vals {
                    let part = format!("{}, ", val.rust_val()?);
                    output.push_str(&part);
                }
                output.push_str("]");
                output
            }
            Struct(name, fields) => {
                let mut output = String::new();
                output.push_str(&format!("{name} {{ "));
                for (idx, field) in fields.iter().enumerate() {
                    let part = format!("{}: {},", FIELD_NAMES[idx], field.rust_val()?);
                    output.push_str(&part);
                }
                output.push_str(" }");
                output
            }
            Float(FloatVal::c_double(val)) => {
                if val.fract() == 0.0 {
                    format!("{val}.0")
                } else {
                    format!("{val}")
                }
            }
            Float(FloatVal::c_float(val)) => {
                if val.fract() == 0.0 {
                    format!("{val}.0")
                } else {
                    format!("{val}")
                }
            }
            Int(int_val) => match int_val {
                c__int128(val) => format!("{val}"),
                c_int64_t(val) => format!("{val}"),
                c_int32_t(val) => format!("{val}"),
                c_int16_t(val) => format!("{val}"),
                c_int8_t(val) => format!("{val}"),
                c__uint128(val) => format!("{val}"),
                c_uint64_t(val) => format!("{val}"),
                c_uint32_t(val) => format!("{val}"),
                c_uint16_t(val) => format!("{val}"),
                c_uint8_t(val) => format!("{val}"),
            },
        };
        Ok(val)
    }

    /// A suitable default value for this type
    fn rust_default_val(&self) -> Result<String, GenerateError> {
        use Val::*;
        let val = match self {
            Ref(x) => x.rust_default_val()?,
            Ptr(_) => format!("0 as *mut ()"),
            Bool(_) => format!("false"),
            Array(vals) => {
                let mut output = String::new();
                output.push_str(&format!("[",));
                for val in vals {
                    let part = format!("{}, ", val.rust_default_val()?);
                    output.push_str(&part);
                }
                output.push_str("]");
                output
            }
            Struct(name, fields) => {
                let mut output = String::new();
                output.push_str(&format!("{name} {{ "));
                for (idx, field) in fields.iter().enumerate() {
                    let part = format!("{}: {},", FIELD_NAMES[idx], field.rust_default_val()?);
                    output.push_str(&part);
                }
                output.push_str(" }");
                output
            }
            Float(..) => format!("0.0"),
            Int(..) => format!("0"),
        };
        Ok(val)
    }

    /// Emit the WRITE calls and FINISHED_VAL for this value.
    /// This will WRITE every leaf subfield of the type.
    /// `to` is the BUFFER to use, `from` is the variable name of the value.
    fn rust_write_val(
        &self,
        to: &str,
        from: &str,
        is_var_root: bool,
    ) -> Result<String, GenerateError> {
        use std::fmt::Write;
        let mut output = String::new();
        for path in self.rust_var_paths(from, is_var_root)? {
            write!(output, "        WRITE.unwrap()({to}, &{path} as *const _ as *const _, core::mem::size_of_val(&{path}) as u32);\n").unwrap();
        }
        write!(output, "        FINISHED_VAL.unwrap()({to});").unwrap();

        Ok(output)
    }

    /// Compute the paths to every subfield of this value, with `from`
    /// as the base path to that value, for rust_write_val's use.
    fn rust_var_paths(&self, from: &str, is_var_root: bool) -> Result<Vec<String>, GenerateError> {
        let paths = match self {
            Val::Int(_) | Val::Float(_) | Val::Bool(_) | Val::Ptr(_) => {
                vec![format!("{from}")]
            }
            Val::Struct(_name, fields) => {
                let mut paths = vec![];
                for (idx, field) in fields.iter().enumerate() {
                    let base = format!("{from}.{}", FIELD_NAMES[idx]);
                    paths.extend(field.rust_var_paths(&base, false)?);
                }
                paths
            }
            Val::Ref(val) => {
                if is_var_root {
                    val.rust_var_paths(from, false)?
                } else {
                    let base = format!("(*{from})");
                    val.rust_var_paths(&base, false)?
                }
            }
            Val::Array(vals) => {
                let mut paths = vec![];
                for (i, val) in vals.iter().enumerate() {
                    let base = format!("{from}[{i}]");
                    paths.extend(val.rust_var_paths(&base, false)?);
                }
                paths
            }
        };

        Ok(paths)
    }
}

impl CallingConvention {
    fn rust_convention_decl(&self) -> &'static str {
        match self {
            CallingConvention::All => {
                unreachable!("CallingConvention::All is sugar that shouldn't reach here")
            }
            CallingConvention::Handwritten => {
                unreachable!("CallingConvention::Handwritten shouldn't reach codegen backends!")
            }
            CallingConvention::C => "C",
            CallingConvention::System => "system",
            CallingConvention::Win64 => "win64",
            CallingConvention::Sysv64 => "sysv64",
            CallingConvention::Aapcs => "aapcs",
            CallingConvention::Stdcall => "stdcall",
            CallingConvention::Fastcall => "fastcall",
            CallingConvention::Vectorcall => "vectorcall",
        }
    }
}
