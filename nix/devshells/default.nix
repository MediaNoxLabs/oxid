{ inputs, ... }:

{
  perSystem =
    { pkgs, self', ... }:
    let
      midnightDidPackages = inputs.midnight-did-toolchain.packages.${pkgs.stdenv.hostPlatform.system};
      linuxLibraries = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.glib
        pkgs.gtk3
        pkgs.libsoup_3
        pkgs.webkitgtk_4_1
        pkgs.xdotool
      ];
      ciRustPackages = with pkgs; [
        cargo
        clippy
        git
        nodejs_24
        pkg-config
        ripgrep
        rustc
        rustfmt
        sccache
      ];
      ciQualityPackages =
        ciRustPackages
        ++ (with pkgs; [
          cargo-audit
          cargo-deny
        ]);
      ciRustShellHook = ''
        export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
        export RUSTC_WRAPPER=${pkgs.sccache}/bin/sccache
        # rustc incremental artifacts are target-directory state and cannot be
        # reused by sccache. CI prefers cross-run object reuse; the interactive
        # developer shell keeps Cargo's normal incremental behavior.
        export CARGO_INCREMENTAL="''${CARGO_INCREMENTAL:-0}"
        export SCCACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/oxid-sccache"
        export SCCACHE_CACHE_SIZE="''${SCCACHE_CACHE_SIZE:-2G}"
      '';
    in
    {
      # Minimal shell for documentation-only checks. It deliberately carries no
      # compilers and none of the Compact/ZK artifact closure that the default
      # shell's environment exports pull in, so Markdown-only workflows never
      # pay for prover-key builds.
      devShells.docs = pkgs.mkShell {
        packages = [
          pkgs.git
          pkgs.lychee
          pkgs.mdbook
          pkgs.mdbook-mermaid
          pkgs.nodejs_24
        ];
      };

      # Hosted Rust lanes deliberately avoid the default developer shell's Pi,
      # Dioxus CLI, Compact toolchain/artifacts, docs, audit, and mobile tools.
      # Each lane adds only the native closure its command actually needs.
      devShells.ci-rust = pkgs.mkShell {
        packages = ciRustPackages;
        buildInputs = [ pkgs.openssl ];
        shellHook = ciRustShellHook;
      };

      devShells.ci-ui = pkgs.mkShell {
        packages = ciRustPackages;
        buildInputs = [ pkgs.openssl ] ++ linuxLibraries;
        shellHook = ''
          ${ciRustShellHook}
          ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath linuxLibraries}:''${LD_LIBRARY_PATH:-}
          ''}
        '';
      };

      devShells.ci-coverage = pkgs.mkShell {
        packages = ciRustPackages ++ [
          pkgs.cargo-llvm-cov
          pkgs.llvmPackages.llvm
        ];
        buildInputs = [ pkgs.openssl ];
        shellHook = ''
          ${ciRustShellHook}
          export LLVM_COV=${pkgs.llvmPackages.llvm}/bin/llvm-cov
          export LLVM_PROFDATA=${pkgs.llvmPackages.llvm}/bin/llvm-profdata
        '';
      };

      # Quality needs audit/deny and rustdoc, but not Pi, Dioxus CLI, Compact
      # artifacts, mdBook/Lychee, mobile tooling, or the default shell's
      # environment-exported proof closures. Do not archive that full shell in
      # GitHub's bounded cache just to run source policy.
      devShells.ci-quality = pkgs.mkShell {
        packages = ciQualityPackages;
        buildInputs = [ pkgs.openssl ] ++ linuxLibraries;
        shellHook = ''
          ${ciRustShellHook}
          ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath linuxLibraries}:''${LD_LIBRARY_PATH:-}
          ''}
        '';
      };

      devShells.default = pkgs.mkShell {
        packages =
          with pkgs;
          [
            cargo
            cargo-audit
            cargo-deny
            cargo-edit
            cargo-llvm-cov
            cargo-nextest
            clippy
            dioxus-cli
            git
            gh
            jq
            just
            lychee
            llvmPackages.llvm
            midnightDidPackages.compact-midnight
            midnightDidPackages.compact-toolchain
            nixfmt
            nodejs_24
            openssl
            pi-coding-agent
            pkg-config
            ripgrep
            rust-analyzer
            rustc
            rustfmt
            sccache
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.xcodegen ];

        buildInputs = linuxLibraries;

        shellHook = ''
                    export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
                    export LLVM_COV=${pkgs.llvmPackages.llvm}/bin/llvm-cov
                    export LLVM_PROFDATA=${pkgs.llvmPackages.llvm}/bin/llvm-profdata
                    export COMPACT_DIRECTORY=${midnightDidPackages.compact-toolchain}
                    export OXID_PRESENTATION_ARTIFACTS_DIR=${self'.packages.presentation-compact-artifacts}
                    export OXID_PASSPORT_VAULT_ARTIFACTS_DIR=${self'.packages.passport-vault-compact-artifacts}
                    export OXID_PASSPORT_VAULT_COMPOSER=${self'.packages.passport-vault-call-composer}/bin/oxid-passport-vault-call-composer
                    # Keep only runtime state in the Git common directory. PI_CODING_AGENT_DIR
                    # remains user-scoped because it owns authentication and user policy;
                    # sessions and pi-subagents lifecycle state are checkout-scoped instead.
                    # This Git-common-dir path survives the per-entry nix-shell TMPDIR and
                    # is private to the local checkout owner, while remaining shared by
                    # its linked worktrees.
                    pi_common_git_dir="$(git rev-parse --path-format=absolute --git-common-dir)"
                    pi_runtime_state_dir="$pi_common_git_dir/oxid-factory/pi-runtime-v1"
                    export PI_CODING_AGENT_SESSION_DIR="$pi_runtime_state_dir/sessions"
                    export PI_SUBAGENTS_TEMP_ROOT="$pi_runtime_state_dir/subagents"
                    mkdir -p "$pi_runtime_state_dir" "$PI_CODING_AGENT_SESSION_DIR" "$PI_SUBAGENTS_TEMP_ROOT"
                    chmod 700 "$pi_runtime_state_dir" "$PI_CODING_AGENT_SESSION_DIR" "$PI_SUBAGENTS_TEMP_ROOT"
                    # Keep one bounded compiler cache across worktrees. Worktree targets
                    # remain isolated for correctness and can be deleted after delivery.
                    export RUSTC_WRAPPER=${pkgs.sccache}/bin/sccache
                    export SCCACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/oxid-sccache"
                    export SCCACHE_CACHE_SIZE="''${SCCACHE_CACHE_SIZE:-10G}"
                    ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
                      export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath linuxLibraries}:''${LD_LIBRARY_PATH:-}
                    ''}

                    # Provision public exact project-local Pi packages without credentials.
                    # CI never needs Pi tooling, and this block performs network package
                    # installs, so continuous-integration shells skip it entirely.
                    if [ -z "''${CI:-}" ] && [ -f .pi/settings.json ]; then
                      # The helper publishes one content-addressed closure only after all
                      # exact pins validate. It migrates a legacy real .pi/npm lazily,
                      # then points this checkout at its matching factory-managed closure.
                      node scripts/factory/provision-pi-packages.mjs
                      # Exact pins were reconciled above. Keep Pi startup itself offline
                      # so it cannot race that authority or retry an unavailable optional
                      # private package. Operators can explicitly unset this for package maintenance.
                      export PI_OFFLINE="''${PI_OFFLINE:-1}"
                    fi
        '';
      };
    };
}
