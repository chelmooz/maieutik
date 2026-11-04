#!/usr/bin/env bash
# gates-ui.sh (v1) — gates GREEN/RED du crate maieutik-ui
# usage : bash gates-ui.sh
# env   : LAYA_MCP_BIN + ORT_DYLIB_PATH (optionnels) activent G5 contre le vrai Laya
#         SKIP_GUI=1 saute la compilation egui (G3/G4)
# GREEN = le code actuel passe ; RED = une mutation volontaire DOIT casser le test visé.
# Le total N est calculé par le script, jamais écrit en dur.
set -u
REPO="${REPO:-$(cd "$(dirname "$0")" && pwd)}"; cd "$REPO" || exit 2
export CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback
LOGS="${GATE_LOGS:-$REPO/../gate-logs/ui-$(date +%Y%m%d-%H%M%S)}"; mkdir -p "$LOGS"
PASS=0; FAIL=0; N=0
gate() { local id="$1" desc="$2"; shift 2; N=$((N+1))
  if "$@" >"$LOGS/$id.log" 2>&1; then echo "GREEN $id — $desc"; PASS=$((PASS+1))
  else echo "FAIL  $id — $desc (voir $LOGS/$id.log)"; FAIL=$((FAIL+1)); fi; }
red() { local id="$1" desc="$2" file="$3" expr="$4" filter="$5"; N=$((N+1))
  cp "$file" "$file.gatebak"; sed -i "$expr" "$file"
  if cmp -s "$file" "$file.gatebak"; then
    mv "$file.gatebak" "$file"; echo "FAIL  $id — mutation sans effet ($desc)"; FAIL=$((FAIL+1)); return; fi
  cargo test -p maieutik-ui --no-default-features -- "$filter" >"$LOGS/$id.log" 2>&1; local rc=$?
  mv "$file.gatebak" "$file"
  if [ $rc -ne 0 ]; then echo "RED✓  $id — $desc : le test détecte la mutation"; PASS=$((PASS+1))
  else echo "FAIL  $id — $desc : mutation NON détectée (test trop faible)"; FAIL=$((FAIL+1)); fi; }
T() { cargo test -p maieutik-ui --no-default-features "$@"; }

gate G1 "build headless"                    cargo build -p maieutik-ui --no-default-features
gate G2 "tests headless (unit+intégration)" T
if [ -z "${SKIP_GUI:-}" ]; then
  gate G3 "build GUI egui (feature gui)"    cargo build -p maieutik-ui --features gui
  gate G4 "fumée GUI sans fenêtre"          cargo test -p maieutik-ui --features gui --test gui_smoke
fi
if [ -n "${LAYA_MCP_BIN:-}" ]; then
  gate G5 "contre le vrai maieutik-mcp"     cargo test -p maieutik-ui --no-default-features --test real_laya -- --ignored --test-threads=1
fi

S=maieutik-ui/src
red R1 "eval/ servi en production"        $S/pipeline.rs 's/h.collection.allowed_in_production() \&\& //' eval_is_excluded_even_if
red R2 "egress : tout hôte accepté"       $S/egress.rs   's/parts.len() == 4/true || parts.len() == 4/;s/h == "localhost" || h == "::1"/true/' remote_hosts_are_refused
red R3 "garde-fou simulé accepté"         $S/pipeline.rs 's/if mock \&\& self.cfg.guard == GuardPolicy::RequireReal/if false \&\& mock/' require_real_guard
red R4 "recherche non routée = erreur"    $S/daemon.rs   's/msg.contains("non implémentée")/false/' unrouted_search
red R5 "pas de fail-closed sans Laya"     $S/pipeline.rs 's/let why = "Laya non configuré — garde-fou requis (fail-closed)".to_owned();/let why = String::new(); return None;/' missing_laya_is_fail_closed
red R6 "bruit stdout fatal"               $S/mcp_client.rs 's/Ok(Line::Noise(s)) => self.noise.push(s),/Ok(Line::Noise(_)) => return Err(PortError::Protocol("bruit".into())),/' stdout_noise_is_counted

echo "----"; echo "gates : $PASS/$N OK, $FAIL échec(s) — journaux : $LOGS"
[ $FAIL -eq 0 ]
