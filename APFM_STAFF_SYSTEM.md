# APFM — Système de staff (mission 9)

## Postes
11 postes (`domain::staff::StaffPost`), avec plafond et minimum requis :

| Poste | Max | Requis |
|---|---|---|
| Entraîneur adjoint | 1 | 1 |
| Entraîneur équipe première | 3 | 1 |
| Préparateur physique | 2 | 1 |
| Entraîneur des gardiens | 2 | 1 |
| Entraîneur des jeunes | 2 | 0 |
| Responsable de la formation | 1 | 0 |
| Recruteur | 6 | 1 |
| Chef recruteur | 1 | 0 |
| Analyste | 2 | 0 |
| Kiné | 3 | 1 |
| Médecin chef | 1 | 0 |

Un poste vacant en dessous du minimum est signalé (« Vacant ») et recommandé par l'adjoint.

## Données FM24
Le staff importé de la save garde son poste (`staff.post`) et ses attributs détaillés FM24 (`staff.detail_json`), stockés par la migration SQL `v060_staff_system.sql` (table `staff_delegation` incluse). Le staff généré n'a pas de détail ; les quatre attributs OFM restent la base.

## Budget et contrats (`ofm_core::staff_system::market`)
- Budget staff = 15 % du budget salarial hebdomadaire du club, plancher 6 000 £/semaine. Un club déjà au-dessus n'est pas forcé de licencier mais ne peut plus ajouter.
- Prime de signature : 1 semaine de salaire. Durée max : 5 ans.
- Salaire demandé : base du membre, majoré jusqu'à +15 % pour un membre fort (niveau 100) dans un club de réputation 0, nul à partir de 700.
- Réponse à une offre : **acceptée** (≥ demande), **contre-proposition** (≥ 90 % de la demande : rien ne change, la réponse porte le salaire acceptable), **refusée** (< 90 %). Accepter une contre-proposition = rappeler avec ce salaire.
- Renouvellement : demande = max(marché, salaire actuel + 5 %).
- Licenciement : indemnité = 50 % des salaires restants, minimum 4 semaines ; payée en trésorerie (journal `ContractTermination`).

## Effets (mesurés, voir `APFM_TRAINING_VALIDATION.md`)
Entraînement (coaching, récupération), progression des jeunes (±8 %) et des gardiens (±12 %), prévention des blessures (jusqu'à 12 %) et durée des blessures (×1,25 → ×0,80), précision du recrutement (analystes), centrés sur 50 : un staff moyen est neutre.

## Commandes Tauri
`get_staff_overview`, `search_staff_candidates`, `propose_staff_contract`, `renew_staff_contract`, `release_staff`, `set_duty_delegation`, `resolve_delegation_proposal`, `get_assistant_recommendations`, `apply_assistant_recommendation` (toutes enregistrées dans `lib.rs`). `hire_staff_internal` est conservée pour l'outil MCP (pas une commande Tauri). L'ancienne commande `hire_staff` n'existe plus : le front passe par `propose_staff_contract`.

## Interface (`src/components/staff/`)
- `StaffTab` : vues Mon staff / Disponibles / **Gestion**. Recrutement et renouvellement via `OfferDialog` (salaire, durée, contre-proposition en un clic, refus, erreurs traduites). Licenciement avec confirmation et montant de l'indemnité. Durée de contrat restante, salaire demandé, blocage éventuel (budget, poste plein).
- `StaffManagementPanel` : budget et postes, délégation, propositions, journal, conseils de l'adjoint.
- i18n : 99 clés (staff.*, assistant.rec.*, delegation.*, be.error.*) dans les 12 langues.

## Points ouverts / à décider
1. **Staff faible pire que pas de staff** pour les jeunes (×0,968 contre ×1,0) et les gardiens (×0,952) : un poste vide est neutre, un membre faible pénalise. Voulu ou à corriger (plancher à 1,0) ? Non modifié.
2. L'ancien front affichait les 4 anciens rôles ; la liste filtre toujours sur ces 4 rôles (`Coach/Scout/Physio/AssistantManager`), pas sur les 11 postes. Le poste réel est affiché sur la fiche « Mon staff ».
3. Les attributs détaillés FM24 ne sont pas encore affichés (fiche staff simple).
