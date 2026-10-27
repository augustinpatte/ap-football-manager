# AP Football Manager — notice de modification (GPL-3.0)

AP Football Manager est une **copie modifiée d'OpenFoot Manager** (https://github.com/openfootmanager/openfootmanager,
base : `develop@f908c2e5`, © les contributeurs d'OpenFoot Manager), distribuée selon les mêmes termes :
**GNU General Public License version 3** (`LICENSE.md`, inchangé).

Ce programme est fourni **sans aucune garantie**. Toute copie distribuée doit rester sous GPL-3.0 et inclure le code
source correspondant ainsi que ce fichier.

## Modifications

L'historique complet est dans le dépôt git (branche `apfm/main`, commits préfixés `feat(apfm)`, `fix(apfm)`, `fix(world)`,
`fix(season-harness)`, `feat(fm24-adapter)`). En résumé :

| Domaine | Fichiers principaux |
|---|---|
| Identité du produit (nom, fenêtre, identifiant d'application, en-tête de page, lettres de marque sans logo) | `src-tauri/tauri.conf.json`, `index.html`, `package.json`, `src/lib/product.ts`, `src/pages/MainMenu.tsx`, `src/components/dashboard/DashboardSidebar.tsx`, `src/i18n/locales/*.json` |
| Import du monde FM24 depuis la sauvegarde de l'utilisateur | `src-tauri/crates/fm24-adapter/`, `src-tauri/src/commands/fm24_world.rs` |
| Choix du monde (FM24 local / démonstration) | `src/components/menu/WorldDatabaseStep.tsx`, `src/services/worldDatabaseService.ts`, `src-tauri/src/commands/world.rs` |
| Aucune donnée envoyée à OFMHub / au projet d'origine (rapports de bogue désactivés, liens communautaires retirés) | `src-tauri/src/report/hub.rs`, `src/lib/product.ts` |
| À propos / licences open source | `src/components/settings/AboutLicences.tsx` |
| Préparation de la synchronisation des sauvegardes entre deux Mac | `src-tauri/src/save_location.rs` |
| Garde-fou : aucune donnée FM24 dans un build ni dans git | `src-tauri/tests/distribution_guard.rs`, `.gitignore` |
| Correctifs de moteur | `src-tauri/crates/ofm_core/src/world/build.rs` (managers d'un monde de référence), `crates/season-harness/src/reputation.rs` |

Les données de Football Manager (marque de Sports Interactive / SEGA) ne sont ni fournies ni distribuées : elles sont lues
dans la sauvegarde de l'utilisateur et restent sur sa machine.
