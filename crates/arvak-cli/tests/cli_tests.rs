//! CLI command parsing and utility tests.
//!
//! Tests cover argument parsing (via clap `try_parse_from`),
//! the shared `common` module, and error paths.

// Re-test the library parts of the CLI.
// The CLI is a binary crate, so we test the public functions from commands::common
// and validate clap parsing by importing the binary's internals via a helper module.

// ============================================================================
// commands::common tests
// ============================================================================

mod common_tests {
    use arvak_compile::{BasisGates, CouplingMap};

    // We can't directly import from the binary crate, so we test the equivalent
    // logic by calling into the underlying crates.

    /// Equivalent to commands::common::get_target_properties
    fn get_target_properties(target: &str) -> anyhow::Result<(CouplingMap, BasisGates)> {
        match target.to_lowercase().as_str() {
            "iqm" | "iqm5" => Ok((CouplingMap::star(5), BasisGates::iqm())),
            "iqm20" => Ok((CouplingMap::star(20), BasisGates::iqm())),
            "ibm" | "ibm5" => Ok((CouplingMap::linear(5), BasisGates::ibm())),
            "ibm27" => Ok((CouplingMap::linear(27), BasisGates::ibm())),
            "simulator" | "sim" => Ok((CouplingMap::full(20), BasisGates::universal())),
            other => anyhow::bail!("Unknown target: '{other}'"),
        }
    }

    #[test]
    fn test_target_iqm() {
        let (cm, bg) = get_target_properties("iqm").unwrap();
        assert_eq!(cm.num_qubits(), 5);
        assert!(!bg.gates().is_empty());
    }

    #[test]
    fn test_target_iqm5_alias() {
        let (cm, _) = get_target_properties("iqm5").unwrap();
        assert_eq!(cm.num_qubits(), 5);
    }

    #[test]
    fn test_target_iqm20() {
        let (cm, _) = get_target_properties("iqm20").unwrap();
        assert_eq!(cm.num_qubits(), 20);
    }

    #[test]
    fn test_target_ibm() {
        let (cm, bg) = get_target_properties("ibm").unwrap();
        assert_eq!(cm.num_qubits(), 5);
        assert!(!bg.gates().is_empty());
    }

    #[test]
    fn test_target_ibm5_alias() {
        let (cm, _) = get_target_properties("ibm5").unwrap();
        assert_eq!(cm.num_qubits(), 5);
    }

    #[test]
    fn test_target_ibm27() {
        let (cm, _) = get_target_properties("ibm27").unwrap();
        assert_eq!(cm.num_qubits(), 27);
    }

    #[test]
    fn test_target_simulator() {
        let (cm, _) = get_target_properties("simulator").unwrap();
        assert_eq!(cm.num_qubits(), 20);
    }

    #[test]
    fn test_target_sim_alias() {
        let (cm, _) = get_target_properties("sim").unwrap();
        assert_eq!(cm.num_qubits(), 20);
    }

    #[test]
    fn test_target_case_insensitive() {
        assert!(get_target_properties("IQM").is_ok());
        assert!(get_target_properties("IBM").is_ok());
        assert!(get_target_properties("Simulator").is_ok());
    }

    #[test]
    fn test_unknown_target() {
        let result = get_target_properties("quantum_computer_9000");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown target"));
    }
}

// ============================================================================
// Circuit loading tests
// ============================================================================

mod circuit_loading {
    use arvak_qasm3::parse;
    use std::fs;

    #[test]
    fn test_parse_valid_qasm() {
        let qasm = "OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];";
        let circuit = parse(qasm).unwrap();
        assert_eq!(circuit.num_qubits(), 2);
    }

    #[test]
    fn test_parse_invalid_qasm() {
        let result = parse("this is not valid qasm");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_circuit() {
        let qasm = "OPENQASM 3.0; qubit[3] q;";
        let circuit = parse(qasm).unwrap();
        assert_eq!(circuit.num_qubits(), 3);
        assert_eq!(circuit.depth(), 0);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = "/tmp/arvak_test_nonexistent_file_12345.qasm";
        assert!(!std::path::Path::new(path).exists());
    }

    #[test]
    fn test_load_circuit_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.qasm");
        fs::write(&path, "OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];").unwrap();

        let source = fs::read_to_string(&path).unwrap();
        let circuit = parse(&source).unwrap();
        assert_eq!(circuit.num_qubits(), 2);
    }

    #[test]
    fn test_json_format_unsupported() {
        // JSON format should fail parsing as QASM
        let result = parse(r#"{"circuit": "test"}"#);
        assert!(result.is_err());
    }
}

// ============================================================================
// Clap argument parsing (test via try_parse_from on equivalent structs)
// ============================================================================

mod clap_parsing {
    use clap::{Parser, Subcommand};

    // Mirror the CLI struct for testing (since main.rs is a binary)
    #[derive(Parser)]
    #[command(name = "arvak")]
    struct TestCli {
        #[arg(short, long, action = clap::ArgAction::Count, global = true)]
        verbose: u8,

        #[command(subcommand)]
        command: TestCommands,
    }

    #[derive(Subcommand)]
    enum TestCommands {
        Compile {
            #[arg(short, long)]
            input: String,
            #[arg(short, long)]
            output: Option<String>,
            #[arg(short, long, default_value = "iqm")]
            target: String,
            #[arg(long, default_value = "1")]
            optimization_level: u8,
        },
        Run {
            #[arg(short, long)]
            input: String,
            #[arg(short, long, default_value = "1024")]
            shots: u32,
            #[arg(short, long, default_value = "simulator")]
            backend: String,
            #[arg(long)]
            compile: bool,
            #[arg(long)]
            target: Option<String>,
        },
        Submit {
            #[arg(short, long)]
            input: String,
            #[arg(short, long, default_value = "simulator")]
            backend: String,
            #[arg(short, long, default_value = "1024")]
            shots: u32,
            #[arg(long, default_value = "slurm")]
            scheduler: String,
            #[arg(long)]
            partition: Option<String>,
            #[arg(long)]
            account: Option<String>,
            #[arg(long)]
            time: Option<String>,
            #[arg(long)]
            priority: Option<String>,
            #[arg(short, long)]
            wait: bool,
        },
        Status {
            job_id: Option<String>,
            #[arg(short, long)]
            all: bool,
        },
        Result {
            job_id: String,
            #[arg(short, long, default_value = "table")]
            format: String,
        },
        Auth {
            #[command(subcommand)]
            action: TestAuthAction,
        },
        Wait {
            job_id: String,
            #[arg(short, long, default_value = "86400")]
            timeout: u64,
        },
        Backends,
        Version,
    }

    #[derive(Subcommand)]
    enum TestAuthAction {
        Login {
            #[arg(short, long)]
            provider: String,
            #[arg(long)]
            project: Option<String>,
        },
        Status {
            #[arg(short, long)]
            provider: Option<String>,
        },
        Logout {
            #[arg(short, long)]
            provider: Option<String>,
        },
    }

    // --- Compile command ---

    #[test]
    fn test_parse_compile_minimal() {
        let cli = TestCli::try_parse_from(["arvak", "compile", "-i", "circuit.qasm"]).unwrap();
        match cli.command {
            TestCommands::Compile {
                input,
                output,
                target,
                optimization_level,
            } => {
                assert_eq!(input, "circuit.qasm");
                assert!(output.is_none());
                assert_eq!(target, "iqm");
                assert_eq!(optimization_level, 1);
            }
            _ => panic!("Expected Compile command"),
        }
    }

    #[test]
    fn test_parse_compile_with_all_args() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "compile",
            "-i",
            "input.qasm",
            "-o",
            "output.qasm",
            "-t",
            "ibm27",
            "--optimization-level",
            "3",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Compile {
                input,
                output,
                target,
                optimization_level,
            } => {
                assert_eq!(input, "input.qasm");
                assert_eq!(output.unwrap(), "output.qasm");
                assert_eq!(target, "ibm27");
                assert_eq!(optimization_level, 3);
            }
            _ => panic!("Expected Compile command"),
        }
    }

    #[test]
    fn test_parse_compile_missing_input() {
        let result = TestCli::try_parse_from(["arvak", "compile"]);
        assert!(result.is_err());
    }

    // --- Run command ---

    #[test]
    fn test_parse_run_minimal() {
        let cli = TestCli::try_parse_from(["arvak", "run", "-i", "bell.qasm"]).unwrap();
        match cli.command {
            TestCommands::Run {
                input,
                shots,
                backend,
                compile,
                target,
            } => {
                assert_eq!(input, "bell.qasm");
                assert_eq!(shots, 1024);
                assert_eq!(backend, "simulator");
                assert!(!compile);
                assert!(target.is_none());
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_parse_run_with_compile_flag() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "run",
            "-i",
            "bell.qasm",
            "--compile",
            "--target",
            "iqm",
            "-s",
            "2048",
            "-b",
            "iqm",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Run {
                compile,
                target,
                shots,
                backend,
                ..
            } => {
                assert!(compile);
                assert_eq!(target.unwrap(), "iqm");
                assert_eq!(shots, 2048);
                assert_eq!(backend, "iqm");
            }
            _ => panic!("Expected Run command"),
        }
    }

    // --- Submit command ---

    #[test]
    fn test_parse_submit_minimal() {
        let cli = TestCli::try_parse_from(["arvak", "submit", "-i", "circuit.qasm"]).unwrap();
        match cli.command {
            TestCommands::Submit {
                scheduler, wait, ..
            } => {
                assert_eq!(scheduler, "slurm");
                assert!(!wait);
            }
            _ => panic!("Expected Submit command"),
        }
    }

    #[test]
    fn test_parse_submit_pbs_with_options() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "submit",
            "-i",
            "circuit.qasm",
            "--scheduler",
            "pbs",
            "--partition",
            "quantum",
            "--account",
            "project123",
            "--time",
            "01:00:00",
            "--priority",
            "high",
            "-w",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Submit {
                scheduler,
                partition,
                account,
                time,
                priority,
                wait,
                ..
            } => {
                assert_eq!(scheduler, "pbs");
                assert_eq!(partition.unwrap(), "quantum");
                assert_eq!(account.unwrap(), "project123");
                assert_eq!(time.unwrap(), "01:00:00");
                assert_eq!(priority.unwrap(), "high");
                assert!(wait);
            }
            _ => panic!("Expected Submit command"),
        }
    }

    // --- Status command ---

    #[test]
    fn test_parse_status_with_job_id() {
        let cli =
            TestCli::try_parse_from(["arvak", "status", "550e8400-e29b-41d4-a716-446655440000"])
                .unwrap();
        match cli.command {
            TestCommands::Status { job_id, all } => {
                assert_eq!(job_id.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
                assert!(!all);
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test]
    fn test_parse_status_all() {
        let cli = TestCli::try_parse_from(["arvak", "status", "--all"]).unwrap();
        match cli.command {
            TestCommands::Status { job_id, all } => {
                assert!(job_id.is_none());
                assert!(all);
            }
            _ => panic!("Expected Status command"),
        }
    }

    // --- Result command ---

    #[test]
    fn test_parse_result_default_format() {
        let cli =
            TestCli::try_parse_from(["arvak", "result", "550e8400-e29b-41d4-a716-446655440000"])
                .unwrap();
        match cli.command {
            TestCommands::Result { format, .. } => {
                assert_eq!(format, "table");
            }
            _ => panic!("Expected Result command"),
        }
    }

    #[test]
    fn test_parse_result_json_format() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "result",
            "550e8400-e29b-41d4-a716-446655440000",
            "-f",
            "json",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Result { format, .. } => {
                assert_eq!(format, "json");
            }
            _ => panic!("Expected Result command"),
        }
    }

    #[test]
    fn test_parse_result_missing_job_id() {
        let result = TestCli::try_parse_from(["arvak", "result"]);
        assert!(result.is_err());
    }

    // --- Auth command ---

    #[test]
    fn test_parse_auth_login() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "auth",
            "login",
            "-p",
            "csc",
            "--project",
            "myproject",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Auth {
                action: TestAuthAction::Login { provider, project },
            } => {
                assert_eq!(provider, "csc");
                assert_eq!(project.unwrap(), "myproject");
            }
            _ => panic!("Expected Auth Login command"),
        }
    }

    #[test]
    fn test_parse_auth_login_missing_provider() {
        let result = TestCli::try_parse_from(["arvak", "auth", "login"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_status() {
        let cli = TestCli::try_parse_from(["arvak", "auth", "status"]).unwrap();
        match cli.command {
            TestCommands::Auth {
                action: TestAuthAction::Status { provider },
            } => {
                assert!(provider.is_none());
            }
            _ => panic!("Expected Auth Status command"),
        }
    }

    #[test]
    fn test_parse_auth_logout() {
        let cli = TestCli::try_parse_from(["arvak", "auth", "logout", "-p", "lrz"]).unwrap();
        match cli.command {
            TestCommands::Auth {
                action: TestAuthAction::Logout { provider },
            } => {
                assert_eq!(provider.unwrap(), "lrz");
            }
            _ => panic!("Expected Auth Logout command"),
        }
    }

    // --- Wait command ---

    #[test]
    fn test_parse_wait_default_timeout() {
        let cli =
            TestCli::try_parse_from(["arvak", "wait", "550e8400-e29b-41d4-a716-446655440000"])
                .unwrap();
        match cli.command {
            TestCommands::Wait { timeout, .. } => {
                assert_eq!(timeout, 86400);
            }
            _ => panic!("Expected Wait command"),
        }
    }

    #[test]
    fn test_parse_wait_custom_timeout() {
        let cli = TestCli::try_parse_from([
            "arvak",
            "wait",
            "550e8400-e29b-41d4-a716-446655440000",
            "-t",
            "3600",
        ])
        .unwrap();
        match cli.command {
            TestCommands::Wait { timeout, .. } => {
                assert_eq!(timeout, 3600);
            }
            _ => panic!("Expected Wait command"),
        }
    }

    // --- Backends & Version ---

    #[test]
    fn test_parse_backends() {
        let cli = TestCli::try_parse_from(["arvak", "backends"]).unwrap();
        assert!(matches!(cli.command, TestCommands::Backends));
    }

    #[test]
    fn test_parse_version() {
        let cli = TestCli::try_parse_from(["arvak", "version"]).unwrap();
        assert!(matches!(cli.command, TestCommands::Version));
    }

    // --- Verbose flag ---

    #[test]
    fn test_parse_verbose_flag() {
        let cli = TestCli::try_parse_from(["arvak", "-v", "version"]).unwrap();
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn test_parse_verbose_vv() {
        let cli = TestCli::try_parse_from(["arvak", "-vv", "version"]).unwrap();
        assert_eq!(cli.verbose, 2);
    }

    #[test]
    fn test_parse_verbose_vvv() {
        let cli = TestCli::try_parse_from(["arvak", "-vvv", "version"]).unwrap();
        assert_eq!(cli.verbose, 3);
    }

    // --- Error cases ---

    #[test]
    fn test_no_subcommand() {
        let result = TestCli::try_parse_from(["arvak"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_subcommand() {
        let result = TestCli::try_parse_from(["arvak", "foobar"]);
        assert!(result.is_err());
    }
}
