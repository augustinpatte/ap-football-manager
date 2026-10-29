# APFM — Délégation et adjoint (mission 9)

## Principe
Six tâches délégables à l'**entraîneur adjoint** (`domain::delegation::Duty`), chacune avec un interrupteur et l'option « me consulter avant les décisions importantes » (`confirm_major`, vrai par défaut) :

| Tâche | Période | Ce que fait l'adjoint |
|---|---|---|
| Entraînement | 1 j | Plan d'entraînement du club (même planificateur que l'IA), récupération avant les périodes chargées |
| Préparation de match | 1 j | Applique les conseils de composition/tactique |
| Recrutement (scouting) | 3 j | Envoie un recruteur observer un joueur |
| Jeunes | 7 j | Promotion d'un jeune prêt (propose si confirmation) |
| Recrutement du staff | 7 j | Comble un poste obligatoire vide ; propose un remplacement net (+12 de niveau) |
| Renouvellements du staff | 7 j | Prolonge un bon membre (niveau ≥ 45) dont le contrat finit sous 60 jours |

Sans adjoint, rien n'est délégable (`be.error.delegation.noAssistant`) ; reprendre la main marche toujours.

## Propositions et journal
Avec `confirm_major`, l'adjoint dépose une **proposition** (durée de vie 21 jours) avec titre, raison et chiffres ; le joueur accepte/refuse (`resolve_delegation_proposal`). Sans, il agit et écrit une ligne au journal. Le journal garde 60 entrées, les propositions 24. L'état est sauvegardé (table `staff_delegation`) ; aller-retour JSON et SQL testés.

## Conseils de l'adjoint
`get_assistant_recommendations` : calcul déterministe depuis l'état du jeu, chaque conseil cite les chiffres qui le justifient. Types : repos, remplacement d'un titulaire (blessé, fatigué, en méforme), poste faible, semaine chargée, ajustement tactique, moral bas, promotion d'un jeune, poste vacant, pas d'adjoint. Un adjoint faible rate les détails ; l'analyste permet de jauger l'adversaire. `apply_assistant_recommendation` retrouve le conseil par id et l'applique tel qu'il serait dit aujourd'hui (`recommendationGone` s'il n'est plus valable).

## Écart avec la décision de délégation du projet principal
`CLAUDE.md` (projet principal) réserve au joueur les négociations de joueurs, transferts, contrats et médias. Ici seuls les contrats **du staff** (embauche, renouvellement) sont délégables à l'adjoint ; transferts, contrats joueurs, tactique et composition restent au joueur (l'adjoint ne fait que proposer). À confirmer : si les renouvellements de staff doivent rester au joueur, c'est une ligne dans `Duty::ALL`.

## Tests
Backend : tests de `staff_system::{delegation,assistant,market,profile}` et `commands::staff` (11). Front : `StaffManagementPanel.test.tsx` (délégation, propositions, conseils, erreur backend, contre-proposition, refus).
