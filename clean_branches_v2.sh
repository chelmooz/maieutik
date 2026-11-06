#!/bin/bash
# =============================================================================
# 🧹 SCRIPT DE NETTOYAGE CORRIGÉ - Version 2.0
# Repo: chelmooz/maieutik
# Objectif: Supprimer toutes les branches GitHub sauf 'main'
# Fix: Affiche les erreurs et gère les permissions GitHub
# =============================================================================

set -euo pipefail

# Couleurs
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}ℹ${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1" >&2; }
warn() { echo -e "${YELLOW}⚠${NC} $1"; }

# =============================================================================
# CONFIGURATION
# =============================================================================

REPO_DIR="/home/chelmooz/Work/maieutik"
cd "$REPO_DIR"

# Liste des branches à supprimer (toutes sauf main)
# audit-a1 n'existe pas sur le remote (seulement a0, a2-a9)
BRANCHES_A_SUPPRIMER=(
    audit-a0 audit-a2 audit-a3 audit-a4 audit-a5 audit-a6 audit-a7 audit-a8 audit-a9
    feat/p0-secrets-model-mgmt
    p1.15 p1.16 p1.17 p1.18 p1.19 p1.20 p1.21 p1.22 p1.23 p1.24
    p1.25 p1.26 p1.27 p1.28 p1.29-aud01 p1.30-idx01 p1.31-idx02
    p1.32-doc01 p1.34-cfg01 p1.35-mcp01 p1.37-perf01 p1.38-mcp-wiring p1.39-index-protection
)

# =============================================================================
# ÉTAPE 1: VÉRIFIER LES PERMISSIONS GITHUB
# =============================================================================

info "============================================================================="
info "🔐 VÉRIFICATION DES PERMISSIONS GITHUB"
info "============================================================================="

if ! gh auth status >/dev/null 2>&1; then
    error "❌ Vous n'êtes pas authentifié sur GitHub CLI !"
    info "Exécutez d'abord: gh auth login"
    info "Ou avec un token: echo 'VOTRE_TOKEN' | gh auth login --with-token"
    exit 1
else
    success "✅ Authentifié sur GitHub CLI"
fi

# Vérifier si on peut accéder au repo
if ! gh api repos/chelmooz/maieutik >/dev/null 2>&1; then
    error "❌ Impossible d'accéder au repo chelmooz/maieutik"
    error "   Vérifiez vos permissions (repo scope nécessaire)"
    exit 1
else
    success "✅ Accès au repo chelmooz/maieutik OK"
fi

# =============================================================================
# ÉTAPE 2: VÉRIFIER L'ÉTAT ACTUEL
# =============================================================================

info ""
info "============================================================================="
info "📊 ÉTAT ACTUEL"
info "============================================================================="

REMOTE_COUNT=$(git branch -r | grep -v "HEAD" | grep "origin/" | wc -l | tr -d ' ')
info "Branches distantes (origin) avant nettoyage: $REMOTE_COUNT"

# =============================================================================
# ÉTAPE 3: SUPPRESSION DES BRANCHES DISTANTES (avec affichage des erreurs)
# =============================================================================

info ""
info "============================================================================="
info "🗑️  SUPPRESSION DES BRANCHES DISTANTES (via gh API)"
info "============================================================================="

SUCCESS_COUNT=0
FAIL_COUNT=0
FAILED_BRANCHES=()

for branch in "${BRANCHES_A_SUPPRIMER[@]}"; do
    info "Suppression de: origin/$branch..."

    if gh api -X DELETE "repos/chelmooz/maieutik/git/refs/heads/$branch" >/dev/null 2>&1; then
        success "✅ Supprimé: $branch"
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        error "❌ ÉCHEC: $branch"
        error "   Cause possible: branche protégée ou permissions insuffisantes"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        FAILED_BRANCHES+=("$branch")
    fi
done

info ""
info "Résultat: $SUCCESS_COUNT réussies, $FAIL_COUNT échecs"

if [ ${#FAILED_BRANCHES[@]} -gt 0 ]; then
    warn "Branches en échec:"
    for b in "${FAILED_BRANCHES[@]}"; do
        warn "  - $b"
    done
fi

# =============================================================================
# ÉTAPE 4: SUPPRESSION DES BRANCHES LOCALES
# =============================================================================

info ""
info "============================================================================="
info "🗑️  SUPPRESSION DES BRANCHES LOCALES"
info "============================================================================="

for branch in "${BRANCHES_A_SUPPRIMER[@]}"; do
    if git show-ref --verify --quiet "refs/heads/$branch" 2>/dev/null; then
        info "Suppression locale de: $branch..."
        if git branch -D "$branch" 2>&1; then
            success "✅ Supprimé local: $branch"
        else
            error "❌ ÉCHEC local: $branch"
        fi
    fi
done

# =============================================================================
# ÉTAPE 5: NETTOYAGE FINAL (fetch prune)
# =============================================================================

info ""
info "============================================================================="
info "🧹 NETTOYAGE FINAL"
info "============================================================================="

git fetch --prune 2>&1 || true
git remote prune origin 2>&1 || true
success "Références nettoyées"

# =============================================================================
# ÉTAPE 6: VÉRIFICATION FINALE
# =============================================================================

info ""
info "============================================================================="
info "✅ VÉRIFICATION FINALE"
info "============================================================================="

AFTER_COUNT=$(git branch -r | grep "origin/" | grep -v "HEAD" | wc -l | tr -d ' ')
info "Branches distantes (origin) après nettoyage: $AFTER_COUNT"

if [ "$AFTER_COUNT" -eq 1 ]; then
    success "✅ SUCCÈS ! Seule 'main' reste sur GitHub"
else
    warn "⚠️  $AFTER_COUNT branches restent (attendu: 1)"
    info "Liste des branches restantes:"
    git branch -r | grep "origin/" | grep -v "HEAD" | sed 's/^/  /'
fi

info ""
info "Vérifiez sur GitHub: https://github.com/chelmooz/maieutik/branches"
info "============================================================================="