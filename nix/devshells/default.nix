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
          # Keep one bounded compiler cache across worktrees. Worktree targets
          # remain isolated for correctness and can be deleted after delivery.
          export RUSTC_WRAPPER=${pkgs.sccache}/bin/sccache
          export SCCACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/oxid-sccache"
          export SCCACHE_CACHE_SIZE=10G
          ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath linuxLibraries}:''${LD_LIBRARY_PATH:-}
          ''}

          # Provision pinned project-local Pi packages. Public packages install
          # without credentials; the optional review package is attempted only
          # when a GitHub token is already available in the user's environment.
          # CI never needs Pi tooling, and this block performs unpinned network
          # installs, so continuous-integration shells skip it entirely.
          if [ -z "''${CI:-}" ] && [ -f .pi/settings.json ]; then
            pi_common_git_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
            if [ -n "$pi_common_git_dir" ]; then
              pi_checkout_root="$(dirname "$pi_common_git_dir")"
            else
              pi_checkout_root="$PWD"
            fi
            if [ -z "''${GITHUB_TOKEN:-}" ]; then
              if [ -n "''${GH_TOKEN:-}" ]; then
                export GITHUB_TOKEN="''${GH_TOKEN}"
              elif [ -n "''${GH_TOKENS:-}" ]; then
                export GITHUB_TOKEN="''${GH_TOKENS}"
              fi
            fi

            while IFS=$'\t' read -r pi_spec pi_package pi_version; do
              [ -n "$pi_spec" ] || continue
              pi_package_json="$pi_checkout_root/.pi/npm/node_modules/$pi_package/package.json"
              pi_installed_version=""
              if [ -f "$pi_package_json" ]; then
                pi_installed_version="$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version ?? "")' "$pi_package_json")"
              fi

              if [ -n "$pi_installed_version" ] && { [ -z "$pi_version" ] || [ "$pi_installed_version" = "$pi_version" ]; }; then
                continue
              fi

              if [ "$pi_package" = "@input-output-hk/agent-review-pi" ] && [ -z "''${GITHUB_TOKEN:-}" ]; then
                echo "Skipping optional Pi package $pi_spec (set GITHUB_TOKEN, GH_TOKEN, or GH_TOKENS to install it)."
                continue
              fi

              echo "Installing project-local Pi package $pi_spec..."
              (cd "$pi_checkout_root" && pi install "$pi_spec" --local --approve </dev/null)
            done < <(node -e '
              const fs = require("fs");
              const settings = JSON.parse(fs.readFileSync(".pi/settings.json", "utf8"));
              for (const spec of settings.packages ?? []) {
                if (typeof spec !== "string" || !spec.startsWith("npm:")) continue;
                const ref = spec.slice(4);
                const at = ref.startsWith("@") ? ref.indexOf("@", 1) : ref.indexOf("@");
                const name = at === -1 ? ref : ref.slice(0, at);
                const version = at === -1 ? "" : ref.slice(at + 1);
                console.log([spec, name, version].join("\t"));
              }
            ')
          fi
        '';
      };
    };
}
