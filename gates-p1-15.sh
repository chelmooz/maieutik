#!/usr/bin/env bash
# gates-p1-15.sh (v2) — gates RED/GREEN du lot P1.15 (codes de sortie, logs, fichiers sans texte)
# usage : bash gates-p1-15.sh pre|post
# env   : REPO (défaut : racine git courante), BIN (binaire à tester, saute le build),
#         SKIP_CARGO=1 (saute C1), SKIP_RED=1 (saute R1), GATE_LOGS (dossier des journaux)
# Chaque gate écrit son journal (trace set -x) dans $GATE_LOGS/<ID>.log.
# Le total N est calculé par le script ; il n'est jamais écrit en dur.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
REPO="${REPO:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$REPO" ] && cd "$REPO" || { echo "REPO introuvable"; exit 2; }
MODE="${1:-}"
VER=VERIFICATION.md

# ---- empreintes de référence (ancrées, jamais recalculées) --------------------
BASE=cb9e1170916797a863784bfe62d838088f148f2e              # = origin/main au départ du lot
INSTALL_SHA=251fb2ddad73ea360f38d339ae7efa8690962dc87366dd53492547b705806c6c
UTIL_SHA_BASE=e3eae32e8a66588514965b5edde9cbf1b161eaf0ae837f1abbc7a69ee9a8afbb
BASE_TESTS_SCAN=7                                          # tests de tests/index_scan.rs à BASE
MIN_NEW_SCAN=6                                             # tests ajoutés à index_scan.rs
MIN_CLI=9                                                  # tests de tests/cli_exit_codes.rs

LOGS="${GATE_LOGS:-$REPO/../gate-logs/$MODE-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$LOGS" || { echo "journaux impossibles : $LOGS"; exit 2; }
echo "journaux : $LOGS"

BIN_GIVEN=""; [ -n "${BIN:-}" ] && BIN_GIVEN=1
BIN="${BIN:-$REPO/target/debug/maieutik}"

# ---- matrice binaire : environnement isolé (env -i), sans modèle --------------
T=""
run_bin() { # run_bin <nom> <cfgdir> <args...>  -> $T/<nom>.{out,err,rc}
  local p="$1" cfg="$2"; shift 2
  ( cd "$T/w" && env -i PATH="$PATH" HOME="$T/h" XDG_CONFIG_HOME="$cfg" XDG_DATA_HOME="$T/d" \
      XDG_CACHE_HOME="$T/h/.cache" MAIEUTIK_EMBEDDER_MODEL="$T/none.onnx" \
      MAIEUTIK_EMBEDDER_TOKENIZER="$T/none.json" \
      timeout 120 "$BIN" "$@" >"$T/$p.out" 2>"$T/$p.err" </dev/null ); echo $? >"$T/$p.rc"
}
collect() {
  T=$(mktemp -d); mkdir -p "$T/w" "$T/c0" "$T/c1/maieutik" "$T/d" "$T/h"
  printf '{"version":1,"settings":{"Folders":["%s"]}}' "$T/w" >"$T/c1/maieutik/config.json"
  run_bin help    "$T/c0" --help
  run_bin bogus   "$T/c0" bogus-subcommand
  run_bin limit0  "$T/c0" search q --limit 0
  run_bin chat    "$T/c0" chat hello
  run_bin model   "$T/c0" model list
  run_bin mcp     "$T/c0" --mcp-stdio
  run_bin idxnone "$T/c0" index            # Folders vide
  run_bin idxnomod "$T/c1" index           # Folders non vide, sans modèle
  run_bin search  "$T/c0" search q
  cp "$T"/*.out "$T"/*.err "$T"/*.rc "$LOGS/" 2>/dev/null
}
rc()   { cat "$T/$1.rc" 2>/dev/null; }
have() { local f; for f in "$@"; do [ -f "$T/$f" ] || { echo "fichier absent : $f"; return 1; }; done; }

g_m_help()    { [ "$(rc help)" = 0 ]; }
g_m_usage()   { [ "$(rc bogus)" = 2 ]; }
g_m_limit0()  { [ "$(rc limit0)" = 2 ]; }
g_m_chat()    { [ "$(rc chat)" = 3 ]; }
g_m_model()   { [ "$(rc model)" = 3 ]; }
g_m_mcp()     { [ "$(rc mcp)" = 3 ]; }
g_m_idxnone() { [ "$(rc idxnone)" = 4 ] && grep -q 'Aucun dossier' "$T/idxnone.err"; }
g_m_idxmod()  { [ "$(rc idxnomod)" = 4 ] && grep -q 'Index indisponible' "$T/idxnomod.err"; }
g_m_search()  { [ "$(rc search)" = 4 ] && grep -q 'Index indisponible' "$T/search.err"; }
ERRS="chat model mcp idxnone idxnomod search"
g_m_stdout()  { local f; for f in $ERRS; do have $f.out $f.err || return 1; [ ! -s "$T/$f.out" ] || { echo "stdout non vide : $f"; return 1; }; done; }
g_m_stderr()  { local f; for f in $ERRS; do have $f.err || return 1; [ -s "$T/$f.err" ] || { echo "stderr vide : $f"; return 1; }; done; }
g_m_ansi()    { local f; have help.out help.err chat.out chat.err search.out search.err || return 1
                ! cat "$T"/*.out "$T"/*.err | grep -q $'\x1b'; }

# ---- état de départ (pre) ------------------------------------------------------
untracked_ok() { git status --porcelain | grep -Ev '^\?\? docs/(INSTALLATION|UTILISATION)\.md$'; }
g_start() {
  [ "$(git rev-parse HEAD)" = "$BASE" ] || { echo "HEAD != BASE"; return 1; }
  [ "$(git rev-parse main)" = "$BASE" ] && [ "$(git rev-parse origin/main)" = "$BASE" ] || { echo "main/origin/main != BASE"; return 1; }
  [ -f src/core/scan.rs ] && ! grep -q 'no_text' src/core/scan.rs || { echo "scan.rs déjà modifié"; return 1; }
  [ -z "$(untracked_ok)" ] || { echo "arbre sale"; untracked_ok; return 1; }
  [ "$(sha256sum docs/INSTALLATION.md | cut -d' ' -f1)" = "$INSTALL_SHA" ] || { echo "INSTALLATION.md différent"; return 1; }
  [ "$(sha256sum docs/UTILISATION.md | cut -d' ' -f1)" = "$UTIL_SHA_BASE" ] || { echo "UTILISATION.md différent"; return 1; }
  [ "$(grep -cE '#\[(tokio::)?test\]' tests/index_scan.rs)" = "$BASE_TESTS_SCAN" ] || { echo "index_scan.rs != $BASE_TESTS_SCAN tests"; return 1; }
  [ "$(grep -cE '^[0-9a-f]{12}  [^ ]+$' "$VER")" = 24 ] || { echo "VERIFICATION != 24 empreintes"; return 1; }
}

# ---- structure du lot ----------------------------------------------------------
g_branch()   { [ "$(git branch --show-current)" = "p1.15" ]; }
g_commits()  { [ "$(git rev-list --count "$BASE"..HEAD)" = 4 ] && [ "$(git merge-base "$BASE" HEAD)" = "$BASE" ]; }
g_msgs()     { local i s; for i in 1 2 3 4; do
                 s=$(git log --reverse --format=%s "$BASE"..HEAD | sed -n "${i}p")
                 case "$s" in "gate(P1.15-$i): "?*) ;; *) echo "sujet $i : $s"; return 1;; esac; done; }
files_of()   { git diff-tree --no-commit-id --name-only -r "$1" | LC_ALL=C sort | paste -sd' '; }
g_perimeter(){ local i c want got; local -a w=(
                 "tests/cli_exit_codes.rs"
                 "src/core/scan.rs src/main.rs tests/index_scan.rs"
                 "README.md docs/UTILISATION.md"
                 "VERIFICATION.md")
               i=0; for c in $(git rev-list --reverse "$BASE"..HEAD); do
                 want="${w[$i]:-?}"; got=$(files_of "$c")
                 [ "$got" = "$want" ] || { echo "commit $((i+1)) : attendu [$want] obtenu [$got]"; return 1; }
                 i=$((i+1)); done; [ "$i" = 4 ]; }
g_nodeps()   { git diff --quiet "$BASE"..HEAD -- Cargo.toml Cargo.lock; }
g_ortenv()   { git diff --quiet "$BASE"..HEAD -- src/core/ort_env.rs; }
g_main()     { [ "$(git rev-parse main)" = "$BASE" ] && [ "$(git rev-parse origin/main)" = "$BASE" ]; }
g_tree()     { [ -z "$(untracked_ok)" ] && [ -z "$(git status --porcelain | grep -E '^\?\? docs/UTILISATION\.md$')" ]; }
g_install()  { [ -f docs/INSTALLATION.md ] \
               && [ "$(sha256sum docs/INSTALLATION.md | cut -d' ' -f1)" = "$INSTALL_SHA" ] \
               && ! git ls-files --error-unmatch docs/INSTALLATION.md >/dev/null 2>&1 \
               && ! git diff --name-only "$BASE"..HEAD | grep -q 'INSTALLATION'; }

# ---- tests ---------------------------------------------------------------------
count_t()    { grep -cE '#\[(tokio::)?test\]' "$1"; }
g_tests_cli(){ [ -f tests/cli_exit_codes.rs ] && [ "$(count_t tests/cli_exit_codes.rs)" -ge "$MIN_CLI" ]; }
g_tests_scan(){ local a b; a=$(git show "$BASE":tests/index_scan.rs | grep -cE '#\[(tokio::)?test\]'); b=$(count_t tests/index_scan.rs)
                echo "avant=$a après=$b"; [ $((b-a)) -ge "$MIN_NEW_SCAN" ]; }
g_tests_exit(){ [ "$(grep -c 'exit_code()' tests/index_scan.rs)" -ge 4 ]; }   # décision 0/1/4 testée sans modèle

# ---- logs ----------------------------------------------------------------------
g_logs()     { grep -q 'with_writer(std::io::stderr)' src/main.rs && grep -q 'is_terminal()' src/main.rs \
               && grep -q 'ort=warn' src/main.rs; }

# ---- documentation -------------------------------------------------------------
g_docs()     { [ "$(grep -cE '^\| [0-4] \|' README.md)" -ge 5 ] && grep -q 'sans texte' README.md \
               && ! grep -q 'Vector index not initialized' README.md \
               && git ls-files --error-unmatch docs/UTILISATION.md >/dev/null 2>&1 \
               && grep -q 'sans texte' docs/UTILISATION.md \
               && ! grep -q 'Vector index not initialized' docs/UTILISATION.md; }
g_verif_text(){ ! grep -qi 'empty stub' "$VER"; }
g_verif_values(){ local n=0 h f; while read -r h f; do n=$((n+1)); [ -f "$f" ] || { echo "absent : $f"; return 1; }
                    [ "$(sha256sum "$f" | cut -c1-12)" = "$h" ] || { echo "empreinte fausse : $f"; return 1; }
                  done < <(grep -E '^[0-9a-f]{12}  [^ ]+$' "$VER"); echo "n=$n"; [ "$n" = 25 ]; }
g_verif_set(){ diff <(grep -E '^[0-9a-f]{12}  [^ ]+$' "$VER" | awk '{print $2}' | sort) \
                    <(git ls-files | grep -E '^(src/|tests/)|^build\.rs$|^Cargo\.(toml|lock)$' | sort); }

# ---- compilation / rouge d'abord ----------------------------------------------
g_cargo()    { [ "${SKIP_CARGO:-0}" = 1 ] && { echo "SAUTÉ"; return 0; }
               cargo check --locked --offline --all-targets && cargo test --locked --offline; }
g_red_c1()   { [ "${SKIP_RED:-0}" = 1 ] && { echo "SAUTÉ"; return 0; }
               local c1 d out r nf; c1=$(git rev-list --reverse "$BASE"..HEAD | head -1); d=$(mktemp -d)
               git worktree add -q --detach "$d" "$c1" || return 1
               out=$( cd "$d" && CARGO_TARGET_DIR="$REPO/target/gate-c1" cargo test --locked --offline --test cli_exit_codes 2>&1 ); r=$?
               git worktree remove --force "$d" >/dev/null 2>&1
               echo "$out" | tail -25
               nf=$(echo "$out" | grep -oE '[0-9]+ failed' | head -1 | cut -d' ' -f1)
               [ $r -ne 0 ] && echo "$out" | grep -q 'test result: FAILED' && ! echo "$out" | grep -q 'could not compile' \
                 && [ "${nf:-0}" -ge 5 ]; }

# ---- exécution -----------------------------------------------------------------
run() { # run <ID> <fn> <description>
  if ( set -x; "$2" ) >"$LOGS/$1.log" 2>&1; then
    if grep -q 'SAUTÉ' "$LOGS/$1.log"; then echo "SKIP   $1 $3  (sauté : ne compte pas comme GREEN)"; return 1; fi
    echo "GREEN  $1 $3"; return 0
  else echo "RED    $1 $3  (journal : $LOGS/$1.log)"; return 1; fi; }

ensure_bin() {
  [ -n "$BIN_GIVEN" ] && { [ -x "$BIN" ] || { echo "BIN inexécutable : $BIN"; exit 2; }; return 0; }
  cargo build --locked --offline >"$LOGS/build.log" 2>&1 || { echo "build impossible :"; tail -30 "$LOGS/build.log"; exit 2; }
}

MATRIX=(
 "M1|g_m_help|--help sort en 0"
 "M2|g_m_usage|sous-commande inconnue sort en 2"
 "M3|g_m_limit0|search --limit 0 sort en 2"
 "M4|g_m_chat|chat sort en 3"
 "M5|g_m_model|model sort en 3"
 "M6|g_m_mcp|--mcp-stdio sort en 3"
 "M7|g_m_idxnone|index, Folders vide : 4 + « Aucun dossier »"
 "M8|g_m_idxmod|index, Folders non vide sans modèle : 4 + « Index indisponible »"
 "M9|g_m_search|search sans modèle : 4 + « Index indisponible »"
 "M10|g_m_stdout|stdout vide sur toutes les erreurs"
 "M11|g_m_ansi|aucun code ANSI hors terminal"
 "M12|g_m_stderr|stderr non vide sur toutes les erreurs"
)
STRUCT=(
 "S1|g_branch|branche p1.15"
 "S2|g_commits|4 commits au-dessus de BASE"
 "S3|g_msgs|sujets gate(P1.15-n) dans l'ordre"
 "S4|g_perimeter|fichiers exacts par commit"
 "S5|g_nodeps|Cargo.toml/Cargo.lock inchangés"
 "S6|g_main|main == origin/main == BASE"
 "S7|g_tree|arbre propre (INSTALLATION.md seul non suivi)"
 "S8|g_install|INSTALLATION.md intact (empreinte), non suivi, hors diff"
 "S9|g_ortenv|ort_env.rs inchangé"
 "L1|g_logs|logs sur stderr, ANSI si terminal, filtre ort=warn"
 "T1|g_tests_cli|tests CLI (>= $MIN_CLI)"
 "T2|g_tests_scan|tests index_scan (+$MIN_NEW_SCAN)"
 "T3|g_tests_exit|décision de sortie testée via exit_code()"
 "D1|g_docs|README + UTILISATION suivi, table 0-4, sans texte"
 "V1|g_verif_text|VERIFICATION : « empty stub » supprimé"
 "V2|g_verif_values|VERIFICATION : 25 empreintes exactes"
 "V3|g_verif_set|VERIFICATION : ensemble de fichiers exact"
 "C1|g_cargo|cargo check + test --locked --offline"
 "R1|g_red_c1|commit 1 rouge par assertion (>= 5 échecs)"
)

case "$MODE" in
  pre)
    run G0 g_start "état de départ (BASE, arbre, empreintes docs, 7 tests, 24 empreintes)" || { echo "STOP : état de départ non conforme"; exit 3; }
    ensure_bin; collect; bad=0
    for e in "${MATRIX[@]}"; do IFS='|' read -r id fn d <<<"$e"
      case "$id" in
        M1|M2) run "$id" "$fn" "$d" || { echo "ANOMALIE (attendu GREEN) : $id"; bad=1; } ;;
        M3|M4|M5|M6|M7|M8|M9) if run "$id" "$fn" "$d" >/dev/null; then echo "ANOMALIE (attendu RED) : $id $d"; bad=1; else echo "RED (attendu) $id $d"; fi ;;
        *) if run "$id" "$fn" "$d" >/dev/null; then echo "GREEN (non déterminant en pre) $id $d"; else echo "RED (non déterminant en pre) $id $d"; fi ;;
      esac
    done
    [ $bad = 0 ] && echo "PRE OK — le worker peut démarrer" || echo "PRE KO — ne pas démarrer, coller la sortie au gatekeeper"
    exit $bad ;;
  post)
    ensure_bin; collect; n=0; tot=0
    for e in "${MATRIX[@]}" "${STRUCT[@]}"; do IFS='|' read -r id fn d <<<"$e"
      tot=$((tot+1)); run "$id" "$fn" "$d" && n=$((n+1)); done
    echo "GREEN=$n/$tot"; [ "$n" = "$tot" ] ;;
  *) echo "usage: bash gates-p1-15.sh pre|post"; exit 2 ;;
esac
