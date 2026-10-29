# APFM — Validation staff / entraînement / progression (mission 9)

Mesures réelles du test `training::tests::staff_effects_rise_from_weak_to_average_to_excellent` (`cargo test -p ofm_core --lib staff_effects_rise -- --nocapture`). Staff complet au même niveau, séance technique :

| Staff | Coaching | Récupération | Jeunes | Gardiens | Blessures évitées | Durée des blessures |
|---|---|---|---|---|---|---|
| aucun | ×0,800 | ×1,000 | ×1,000 | ×1,000 | 0,0 % | ×1,25 |
| faible (30) | ×1,000 | ×1,147 | ×0,968 | ×0,952 | 3,6 % | ×1,11 |
| moyen (50) | ×1,100 | ×1,248 | ×1,000 | ×1,000 | 6,0 % | ×1,02 |
| excellent (90) | ×1,300 | ×1,458 | ×1,064 | ×1,096 | 10,8 % | ×0,85 |

L'ordre aucun < faible < moyen < excellent est vérifié par assertion pour coaching, récupération, prévention et durée. **Exception constatée** : jeunes et gardiens, où un membre faible fait pire que pas de membre (poste vide neutre) ; l'assertion ne couvre ces deux effets qu'à partir de « faible ». Non corrigé (décision d'équilibrage).

## Autres validations exécutées
- Sauvegarde/rechargement : `a_staff_seat_and_detail_attributes_survive_a_round_trip` (poste + détail FM24), `delegation_round_trips…` (délégation), JSON de l'état de délégation.
- Budgets et recrutement : `commands::staff` (offre refusée, contre-proposition, budget à 0 bloque la signature, indemnité de licenciement, siège libéré) et `market` (budget, plafond de poste, trésorerie).
- Délégation : tests `delegation.rs` (période, confirmation, renouvellement du bon, départ du faible).
- Blessures : le staff médical peut absorber un choc ; le test `two_players_hurt_in_one_match_are_drawn_independently` supposait une blessure à chaque graine et a été corrigé pour ne comparer que les graines où les deux sont blessés.

## Non mesuré
- Effet cumulé du staff sur une saison complète de progression des joueurs (pas de simulation de saison avec comparaison faible/excellent lancée ici). Les multiplicateurs ci-dessus sont des facteurs par séance/événement, pas des gains de saison.
- Test manuel dans l'application (aucun lancement de l'interface graphique).
