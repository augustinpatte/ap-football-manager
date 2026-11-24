# APFM INTEGRATION5 — Real FM 2026/27 European competitions: the integration audit

**Verdict: GO for the independent audit and for the owner's scratch-world acceptance test, with the limits in section 17 (GUI not operated; calendar tail delays; access list and coefficients modelled, not UEFA's)**

The three European competitions of 2026/27 exist in the locally built FM world with UEFA's verified 36
participants each, a manager with Paris Saint-Germain (or any other club) plays them through the same
workflow as domestic football, they are scheduled on UEFA's published dates, settled, archived and
renewed, and the 2027/28 fields follow from the simulated results. This is **not** a claim of full
UEFA compliance (section 17), the GUI was **not** operated (section 16), and nothing here was
installed into the owner's world.

## 1. Baseline and final commits

* Baseline `4ad6cfa7ca0dec9a68db9b93678f5bf94e019e43` (`apfm/main`; local == origin; clean tree at the start).
* Final: **the commit that adds this document (reported with the push; the pushed HEAD)**.
* Commits on top, oldest first: 
  * `82a25eba` fix(league-phase): published EL/ECL calendars, no invented association cap, neutral final
  * `d04f4f36` feat(europe): real 2026/27 European fields, continental-only clubs and qualification rules
  * `048a0b6a` feat(europe): manager-facing European messages, notes and neutral-final marker; coefficients seed later editions
  * `b495588c` feat(calendar): keep first-division midweek rounds off the European weeks; harness studies
  * `2e47e757` fix(europe): reviewer findings — berths wired, names translated, final once, size caps
  * `44834177` fix(berths): a club picked to fill one field is placed for the next
  * `46290036` fix(harness): shared rollover definition, European invariants, stronger real-world checks
  * `cceeffdb` fix(adapter): the registry treats European-only clubs' players as active-world players
  * this document (the final commit, whose SHA is the one reported with the push)

## 2. Source database integrity

`~/apfootballmanager/data/world.db` was opened only read-only (the adapter's
`SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only`; `mode=ro&immutable=1` for inspection).

| | SHA-256 |
|---|---|
| Before the mission | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |
| After the mission | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |

**Identical: yes, byte-identical (the adapter's own before/after hash check passed in every import).** The adapter also hashes the file before and after every import
(`source.untouched`, passed in every run). The installed world, personal careers and saves were never
opened. Every world built for this mission lives in the session scratch directory; nothing derived
from the private database (squads, exports, saves) is in the repository. The one source-derived file
that is committed is `config/europe_2026_27.csv`: 108 rows of UEFA name, OFM association code, source
club id and name, city and the matching evidence, the same kind of id/name/city table the repository
already carries as `config/club_city.csv`. It carries no player, rating, wage or finance data.

## 3. The authoritative 2026/27 reference (Step 2)

Read on **2026-11-24**. UEFA's own pages (dated 4 and 23 November 2026) mark every club list
*provisional, based purely on sporting performance, pending licensing and disciplinary matters* and
every date *subject to change*.

| Source | What it gave |
|---|---|
| uefa.com Champions League 2026/27 teams, dates, format (4 Nov 2026) | 36 clubs; matchdays, play-offs, R16, QF, SF; final Sat 5 Jun 2027, Estadio Metropolitano, Madrid |
| uefa.com Europa League 2026/27 (23 Nov 2026) | 36 clubs, 22 associations; dates; final 26 May 2027, Stadion Frankfurt |
| uefa.com Conference League 2026/27 (23 Nov 2026) | 36 clubs, 30 associations; dates; final 2 Jun 2027, Beşiktaş Stadium, Istanbul |
| Regulations of the UEFA Champions League 2026/27, Annex B (two diagrams) and Article 19 | Read **from the images**: the knockout bracket; 19.04, 19.05 (silver side = nominal home team of the final) |
| uefa.com club-coefficient methodology (Annex D) | The method only |

### 3.1 Reference matrix

| Fact | Value | Status |
|---|---|---|
| Fields | CL 36 (England 5, Spain 5, Germany 4, Italy 4, France 3, Netherlands 2, Norway 2, Portugal 2, Türkiye 2, one each Austria, Azerbaijan, Belgium, Czechia, Greece, Slovakia, Ukraine); EL 36; ECL 36 | **Confirmed by UEFA** (provisional) |
| CL league phase | 8-10 Sep, 13/14 Oct, 20/21 Oct, 3/4 Nov, 24/25 Nov, 8/9 Dec 2026, 19/20 Jan, 27 Jan 2027; play-offs 16/17 & 23/24 Feb; R16 9/10 & 16/17 Mar; QF 6/7 & 13/14 Apr; SF 27/28 Apr & 4/5 May; final 5 Jun | **Confirmed** (unchanged from EUROPE4) |
| EL league phase | 16/17 Sep, 15 Oct, 22 Oct, 5 Nov, 26 Nov, 10 Dec 2026, 21 Jan, 28 Jan 2027; play-offs 18/25 Feb; R16 11/18 Mar; QF 8/15 Apr; SF 29 Apr/6 May; final 26 May | **Confirmed**; brief re-verified, matched |
| ECL league phase | 15 Oct, 22 Oct, 5 Nov, 26 Nov, 10 Dec, 17 Dec 2026; play-offs 18/25 Feb; R16 11/18 Mar; QF 8/15 Apr; SF 29 Apr/6 May; final 2 Jun | **Confirmed**; brief re-verified, matched |
| Bracket (Annex B) | section 9 | **Confirmed** from the diagram |
| Pots, draw, coefficients, prize distribution, access list | not on the pages / not retrievable | **Not confirmed**: APFM assumptions below, each labelled in the game |

The pages show no draw date and no pots. The UEFA coefficients table did not load (only the methodology
article); the Wikipedia tournament pages give association, not club, coefficients. **No coefficient,
pot, draw, fixture or result was invented or labelled official.** No simulated outcome is pre-filled
from real results.

## 4. Integration map (Step 1)

| Concern | Production path | Finding |
|---|---|---|
| World import and definitions | `fm24-adapter/{source,pipeline,competitions,europe}.rs` → package files → `ofm_core::generator::build_world_from_package_seeded` | The source's three continental competitions hold **no participants**; the fields come from UEFA through `config/europe_2026_27.csv`. The pipeline's perimeter is still the 240 domestic clubs; the European-only clubs are added through the same placement/rating/finance pipeline |
| Club and player identity | `ids.rs` (`fm-c-*`, `fm-p-*`, `eu-gen-*`) | Stable across seasons and saves |
| Active world vs registry | `registry.rs` (a separate SQLite projection of external players) | The registry never held clubs; the 69 continental clubs' 3,056 players are now active-world players, so there is no second owner |
| Berth resolution | `end_of_season/berths/continental.rs` | Existing priority, exclusivity and `fallbackTo` kept; three rules added (section 7) |
| Draw and fixtures | `league_phase/{draw,matchdays,calendar,pots,profiles}.rs` | Drawn when the game is built (`draw_pending_editions`): imported worlds previously never drew their league phases |
| Match simulation and finalisation | `live_match_manager`, `turn::post_match`, `competition_flow::after_result` | One engine; one dispatch; exactly-once guard `fixture_is_open` |
| Calendar | `calendar_booking`, `calendar_reconcile`, `schedule_pattern` | Continental is the top club layer; domestic rounds are planned around European weeks (section 10) |
| Completion, rollover, renewal | `competition_completion`, `end_of_season`, `turn/cup_renewal` | A finished edition waits for a sibling still being played, at the rollover and in the daily renewal |
| Persistence | `db/repositories/competition_repo` (JSON columns) | No migration needed: new fields are `serde(default)` inside existing JSON columns (section 13) |
| Frontend | `components/tournaments/LeaguePhase*`, inbox, news | Section 12 |

## 5. Club-by-club coverage (Step 4)

Matching rule: **the source's internal club id, with the source's own name kept beside it**. The
importer **refuses** a row whose id carries another name (`europe::check_against_source`, tested by
`a_renamed_source_club_is_reported_not_followed`) and checks the association against the club's nation.
Ambiguous cases carry their reason: UEFA "Sporting CP" is FM 962 *Sporting Clube de Portugal*, **not**
FM 961 *Sporting Clube de Braga* (that is UEFA "Braga"); "CSKA Sofia" is FM 15218 *CSKA-Sofia*, not
FM 15220 *CSKA 1948 Sofia* (medium confidence); "Kauno Žalgiris" is FM 24016, not *Žalgiris Vilnius*.

| Result | Clubs |
|---|---|
| Matched to a club of the 240-club domestic perimeter | 38 |
| Matched to a source club **outside** the perimeter, added as a continental-only team | 69 |
| **No source counterpart**: generated club under its UEFA name | 1 (Iberia Tbilisi: the only candidate, *SK Iberia-2010 Tbilisi*, 0 players, a different founding year) |
| Source club found but with **no nation** in `world.db` (Gibraltar is not in its nation table) | 1 (Lincoln Red Imps, id 18218; association `GI` from UEFA, recorded in the evidence) |

The 69 continental-only clubs carry **3,056 imported players** and 2 generated fillers; every club has
at least 15 seniors. Four clubs have no head coach in the source (CSKA Sofia, Monaco, Kairat, and the
generated Iberia Tbilisi); OFM appoints a generated manager as for any club without one.

### 5.1 All 108 participants

| Comp | UEFA name | Assoc | FM club | FM name (world.db) | Status | Imported / generated players | Staff | Manager |
|---|---|---|---|---|---|---|---|---|
| UCL | LASK | AT | 38 | Linzer ASK | continental-only team | 43 / 0 | 17 | real head coach |
| UCL | Sabah | AZ | 27327 | Sabah FK | continental-only team | 26 / 0 | 6 | real head coach |
| UCL | Club Brugge | BE | 57 | Club Brugge KV | continental-only team | 40 / 0 | 36 | real head coach |
| UCL | Slavia Praha | CZ | 238 | SK Slavia Praha | continental-only team | 54 / 0 | 31 | real head coach |
| UCL | Arsenal | ENG | 321 | Arsenal | domestic league (240 perimeter) | 61 / 0 | 58 | real head coach |
| UCL | Aston Villa | ENG | 322 | Aston Villa | domestic league (240 perimeter) | 63 / 0 | 47 | real head coach |
| UCL | Liverpool | ENG | 387 | Liverpool | domestic league (240 perimeter) | 73 / 0 | 71 | real head coach |
| UCL | Manchester City | ENG | 389 | Manchester City | domestic league (240 perimeter) | 59 / 0 | 59 | real head coach |
| UCL | Manchester United | ENG | 390 | Manchester United | domestic league (240 perimeter) | 69 / 0 | 63 | real head coach |
| UCL | Lens | FR | 501 | RC Lens | domestic league (240 perimeter) | 49 / 0 | 25 | real head coach |
| UCL | Lille | FR | 489 | LOSC Lille | domestic league (240 perimeter) | 65 / 0 | 32 | real head coach |
| UCL | Paris Saint-Germain | FR | 498 | Paris Saint-Germain | domestic league (240 perimeter) | 69 / 0 | 32 | real head coach |
| UCL | Bayern München | DE | 528 | FC Bayern München | domestic league (240 perimeter) | 90 / 0 | 44 | real head coach |
| UCL | Borussia Dortmund | DE | 523 | Borussia Dortmund | domestic league (240 perimeter) | 66 / 0 | 32 | real head coach |
| UCL | RB Leipzig | DE | 34313 | RasenBallsport Leipzig | domestic league (240 perimeter) | 69 / 0 | 41 | real head coach |
| UCL | VfB Stuttgart | DE | 559 | VfB Stuttgart | domestic league (240 perimeter) | 47 / 0 | 34 | real head coach |
| UCL | AEK Athens | GR | 563 | AEK Athens | continental-only team | 54 / 0 | 20 | real head coach |
| UCL | Como | IT | 689 | Como 1907 | domestic league (240 perimeter) | 56 / 0 | 43 | real head coach |
| UCL | Inter | IT | 699 | F.C. Internazionale Milano | domestic league (240 perimeter) | 64 / 0 | 69 | real head coach |
| UCL | Napoli | IT | 711 | S.S.C. Napoli | domestic league (240 perimeter) | 54 / 0 | 40 | real head coach |
| UCL | Roma | IT | 670 | A.S. Roma | domestic league (240 perimeter) | 62 / 0 | 44 | real head coach |
| UCL | Feyenoord | NL | 601 | Feyenoord Rotterdam | continental-only team | 93 / 0 | 52 | real head coach |
| UCL | PSV | NL | 614 | PSV Eindhoven | continental-only team | 86 / 0 | 51 | real head coach |
| UCL | Bodø/Glimt | NO | 813 | FK Bodø/Glimt | continental-only team | 44 / 0 | 24 | real head coach |
| UCL | Viking | NO | 921 | Viking FK | continental-only team | 43 / 0 | 15 | real head coach |
| UCL | Porto | PT | 952 | Futebol Clube do Porto | continental-only team | 78 / 0 | 32 | real head coach |
| UCL | Sporting CP | PT | 962 | Sporting Clube de Portugal | continental-only team | 89 / 0 | 34 | real head coach |
| UCL | Slovan Bratislava | SK | 1051 | SK Slovan Bratislava | continental-only team | 28 / 0 | 22 | real head coach |
| UCL | Atlético de Madrid | ES | 1088 | Club Atlético de Madrid | domestic league (240 perimeter) | 65 / 0 | 34 | real head coach |
| UCL | Barcelona | ES | 1099 | F.C. Barcelona | domestic league (240 perimeter) | 61 / 0 | 40 | real head coach |
| UCL | Real Betis | ES | 1112 | Real Betis Balompié | domestic league (240 perimeter) | 53 / 0 | 31 | real head coach |
| UCL | Real Madrid | ES | 1113 | Real Madrid C.F. | domestic league (240 perimeter) | 63 / 0 | 57 | real head coach |
| UCL | Villarreal | ES | 1135 | Villarreal C.F. | domestic league (240 perimeter) | 43 / 0 | 36 | real head coach |
| UCL | Fenerbahçe | TR | 1206 | Fenerbahçe A.Ş. | continental-only team | 42 / 0 | 27 | real head coach |
| UCL | Galatasaray | TR | 1207 | Galatasaray A.Ş. | continental-only team | 31 / 0 | 38 | real head coach |
| UCL | Shakhtar Donetsk | UA | 1221 | Shakhtar Donetsk | continental-only team | 50 / 0 | 18 | real head coach |
| UEL | Ararat-Armenia | AM | 27333 | Ararat-Armenia Yerevan | continental-only team | 27 / 0 | 6 | real head coach |
| UEL | Salzburg | AT | 42 | FC Red Bull Salzburg | continental-only team | 47 / 0 | 39 | real head coach |
| UEL | Sturm Graz | AT | 40 | SK Puntigamer Sturm Graz | continental-only team | 46 / 0 | 22 | real head coach |
| UEL | Anderlecht | BE | 109 | Royal Sporting Club Anderlecht | continental-only team | 66 / 0 | 35 | real head coach |
| UEL | Union SG | BE | 128 | Royale Union Saint-Gilloise | continental-only team | 37 / 0 | 17 | real head coach |
| UEL | Levski Sofia | BG | 174 | PFC Levski Sofia | continental-only team | 41 / 0 | 12 | real head coach |
| UEL | Dinamo Zagreb | HR | 205 | GNK Dinamo Zagreb | continental-only team | 63 / 0 | 36 | real head coach |
| UEL | Omonia | CY | 227 | Omonoia Nicosia | continental-only team | 32 / 0 | 13 | real head coach |
| UEL | Sparta Praha | CZ | 240 | AC Sparta Praha | continental-only team | 46 / 0 | 26 | real head coach |
| UEL | Viktoria Plzeň | CZ | 241 | FC Viktoria Plzeň | continental-only team | 52 / 0 | 23 | real head coach |
| UEL | Bournemouth | ENG | 319 | AFC Bournemouth | domestic league (240 perimeter) | 47 / 0 | 31 | real head coach |
| UEL | Crystal Palace | ENG | 358 | Crystal Palace | domestic league (240 perimeter) | 47 / 0 | 45 | real head coach |
| UEL | Sunderland | ENG | 429 | Sunderland | domestic league (240 perimeter) | 37 / 0 | 36 | real head coach |
| UEL | Lyon | FR | 495 | Olympique Lyonnais | domestic league (240 perimeter) | 60 / 0 | 35 | real head coach |
| UEL | Marseille | FR | 496 | Olympique de Marseille | domestic league (240 perimeter) | 55 / 0 | 37 | real head coach |
| UEL | Rennes | FR | 511 | Stade Rennais FC | domestic league (240 perimeter) | 49 / 0 | 36 | real head coach |
| UEL | Hoffenheim | DE | 5825 | TSG 1899 Hoffenheim | domestic league (240 perimeter) | 39 / 0 | 25 | real head coach |
| UEL | Leverkusen | DE | 519 | Bayer 04 Leverkusen | domestic league (240 perimeter) | 37 / 0 | 50 | real head coach |
| UEL | OFI Crete | GR | 575 | OFI Crete | continental-only team | 46 / 0 | 12 | real head coach |
| UEL | Olympiacos | GR | 576 | Olympiacos SF Piraeus | continental-only team | 55 / 0 | 28 | real head coach |
| UEL | Ferencváros | HU | 635 | Ferencvárosi Torna Club | continental-only team | 37 / 0 | 27 | real head coach |
| UEL | Hapoel Be'er Sheva | IL | 657 | Hapoel Be'er-Sheva | continental-only team | 39 / 0 | 15 | real head coach |
| UEL | Milan | IT | 669 | A.C. Milan | domestic league (240 perimeter) | 70 / 0 | 62 | real head coach |
| UEL | Juventus | IT | 702 | Juventus F.C. | domestic league (240 perimeter) | 92 / 0 | 65 | real head coach |
| UEL | AZ Alkmaar | NL | 584 | AZ Alkmaar | continental-only team | 79 / 0 | 39 | real head coach |
| UEL | N.E.C. | NL | 611 | N.E.C. Nijmegen | continental-only team | 66 / 0 | 19 | real head coach |
| UEL | Lillestrøm | NO | 854 | Lillestrøm SK | continental-only team | 30 / 0 | 12 | real head coach |
| UEL | Jagiellonia | PL | 4646 | Jagiellonia Białystok | continental-only team | 36 / 0 | 20 | real head coach |
| UEL | Lech Poznań | PL | 940 | Lech Poznań | continental-only team | 52 / 0 | 31 | real head coach |
| UEL | Benfica | PT | 960 | Sport Lisboa e Benfica | continental-only team | 74 / 0 | 32 | real head coach |
| UEL | Torreense | PT | 1590 | Sport Clube União Torreense | continental-only team | 28 / 0 | 7 | real head coach |
| UEL | Celtic | SCO | 1014 | Celtic | continental-only team | 58 / 0 | 24 | real head coach |
| UEL | Celje | SI | 1063 | NK Celje | continental-only team | 37 / 0 | 16 | real head coach |
| UEL | Real Sociedad | ES | 1119 | Real Sociedad de Fútbol | domestic league (240 perimeter) | 31 / 0 | 25 | real head coach |
| UEL | Celta | ES | 1106 | R.C. Celta de Vigo | domestic league (240 perimeter) | 30 / 0 | 35 | real head coach |
| UEL | Beşiktaş | TR | 1203 | Beşiktaş A.Ş. | continental-only team | 36 / 0 | 27 | real head coach |
| UECL | Egnatia | AL | 10680 | Klubi i Futbollit Egnatia Rrogozhinë | continental-only team | 23 / 0 | 4 | real head coach |
| UECL | Inter Club d'Escaldes | AD | 2221 | Inter Club d'Escaldes | continental-only team | 23 / 1 | 6 | real head coach |
| UECL | Gent | BE | 48 | KAA Gent | continental-only team | 38 / 0 | 31 | real head coach |
| UECL | Sint-Truiden | BE | 121 | K Sint-Truidense VV | continental-only team | 29 / 0 | 14 | real head coach |
| UECL | Borac Banja Luka | BA | 6491 | FK Borac Banja Luka | continental-only team | 35 / 0 | 9 | real head coach |
| UECL | CSKA Sofia | BG | 15218 | CSKA-Sofia | continental-only team | 34 / 0 | 20 | none |
| UECL | Hajduk Split | HR | 206 | HNK Hajduk Split | continental-only team | 62 / 0 | 42 | real head coach |
| UECL | Pafos | CY | 16401 | Pafos FC | continental-only team | 27 / 0 | 19 | real head coach |
| UECL | Jablonec | CZ | 233 | FK Jablonec | continental-only team | 27 / 0 | 10 | real head coach |
| UECL | Aarhus | DK | 245 | Aarhus Gymnastikforening | continental-only team | 31 / 0 | 16 | real head coach |
| UECL | Copenhagen | DK | 261 | Football Club København | continental-only team | 38 / 0 | 25 | real head coach |
| UECL | Midtjylland | DK | 277 | Football Club Midtjylland | continental-only team | 41 / 0 | 36 | real head coach |
| UECL | Nordsjælland | DK | 6189 | Football Club Nordsjælland | continental-only team | 42 / 0 | 25 | real head coach |
| UECL | Brighton | ENG | 336 | Brighton & Hove Albion | domestic league (240 perimeter) | 56 / 0 | 46 | real head coach |
| UECL | KuPS | FI | 460 | Kuopion Palloseura | continental-only team | 31 / 0 | 12 | real head coach |
| UECL | Monaco | FR | 466 | AS Monaco | domestic league (240 perimeter) | 60 / 0 | 46 | none |
| UECL | Iberia Tbilisi | GE | — | — | GENERATED (no source club) | 0 / 15 | 4 | none |
| UECL | Freiburg | DE | 547 | Sport-Club Freiburg | domestic league (240 perimeter) | 28 / 0 | 22 | real head coach |
| UECL | Lincoln Red Imps | GI | 18218 | Lincoln Red Imps FC | continental-only team | 38 / 0 | 11 | real head coach |
| UECL | Panathinaikos | GR | 578 | Panathinaikos AO | continental-only team | 52 / 0 | 25 | real head coach |
| UECL | Atalanta | IT | 675 | Atalanta Bergamasca Calcio | domestic league (240 perimeter) | 101 / 0 | 66 | real head coach |
| UECL | Kairat | KZ | 4009 | Qairat Almaty | continental-only team | 16 / 1 | 9 | none |
| UECL | Riga | LV | 24042 | Riga FC | continental-only team | 29 / 0 | 16 | real head coach |
| UECL | Kauno Žalgiris | LT | 24016 | FK Kauno Žalgiris | continental-only team | 26 / 0 | 12 | real head coach |
| UECL | Ajax | NL | 585 | AFC Ajax | continental-only team | 93 / 0 | 14 | real head coach |
| UECL | Twente | NL | 597 | FC Twente | continental-only team | 56 / 0 | 24 | real head coach |
| UECL | Brann | NO | 882 | SK Brann | continental-only team | 37 / 0 | 22 | real head coach |
| UECL | Braga | PT | 961 | Sporting Clube de Braga | continental-only team | 59 / 0 | 37 | real head coach |
| UECL | Universitatea Craiova | RO | 26626 | U Craiova 1948 CS | continental-only team | 32 / 0 | 20 | real head coach |
| UECL | Hearts | SCO | 1018 | Heart Of Midlothian | continental-only team | 55 / 0 | 22 | real head coach |
| UECL | Crvena Zvezda | RS | 1267 | FK Crvena zvezda Beograd | continental-only team | 38 / 0 | 35 | real head coach |
| UECL | Getafe | ES | 1101 | Getafe C.F. | domestic league (240 perimeter) | 37 / 0 | 21 | real head coach |
| UECL | Mjällby | SE | 1165 | Mjällby AIF | continental-only team | 31 / 0 | 9 | real head coach |
| UECL | Lugano | CH | 1190 | FC Lugano | continental-only team | 24 / 0 | 23 | real head coach |
| UECL | Thun | CH | 4099 | FC Thun | continental-only team | 23 / 0 | 15 | real head coach |
| UECL | Trabzonspor | TR | 1213 | Trabzonspor A.Ş. | continental-only team | 35 / 0 | 32 | real head coach |


## 6. How a club outside the perimeter plays (Step 5)

*Is a lightweight continental-only club viable?* **No.** The engine needs a complete `Team` with
players, a manager and a squad at the floor, and the daily loop (training, finance, AI squad management,
injuries) treats every team alike. The smallest reliable way is the one used: **an ordinary team**,
built by the same pipeline, with: no domestic league (`europe.no_domestic_league`), its real association
as `football_nation`, its source identity as id, no possibility of promotion, relegation or a domestic
prize, and a generated squad and manager only where the source has none (listed in the import report).
The harness's pyramid rule knows a nation whose clubs exist only for Europe has no pyramid to lose, and
still reports a deleted league (`a_nation_that_exists_only_for_europe_has_no_pyramid_to_lose`).

## 7. Fields, the association cap, and later seasons (Steps 3B, 6, 7)

### Blocker B — the invented cap

EUROPE4 capped a whole edition at four (three) clubs per association as a *stand-in for access lists*.
The real 2026/27 Champions League has five English and five Spanish clubs. Separated:

| Concern | Now |
|---|---|
| Whole-edition association limit | **Removed** from the shipped profiles (`0` = no limit; not a UEFA rule). A non-zero value still validates and the synthetic harness keeps an explicit one |
| Association access list | Authored berths (below) |
| Draw-level restrictions | Unchanged: no same-association pair, at most two opponents from one other association; an independent `verify` re-checks every draw |
| Per-pot feasibility | A pot with more than half its places from one association cannot be drawn. The smallest explicit relaxation: when no draw exists, the crowded association's lowest-ranked club trades pots with the nearest-ranked club of another association in the neighbouring pot, one trade at a time, **recorded** in `LeaguePhaseState.pot_adjustments` and shown in the UI. No pairing rule is ever relaxed |

*Evidence:* the complete 2026/27 association layouts (CL, EL, ECL) drawn under **300 random rankings each
(900 draws)**: all drawn and verified, fewer than 30 needed a trade, none more than two
(`the_real_2026_27_fields_draw_under_every_ranking_without_a_cap`); a clustered ranking is repaired
through `regenerate_for_season` and the trade is recorded (`a_crowded_pot_is_traded_and_the_trade_is_recorded_in_the_edition`).
In the real world the first draws needed no trade.

### The first season (a verified snapshot)

The fields are UEFA's. The draw is **the game's own deterministic, seeded draw**, made when the game is
built, never labelled UEFA's (UI note and inbox). PSG is a participant by the same rule as every club.
Pots are by club reputation (`coefficient_fallback_used`; section 8).

### Seasons after the first

Results decide, through the existing berth machinery (priority, exclusivity, `fallbackTo`, vacancy
refill) plus three rules:

| Rule | Meaning |
|---|---|
| `EuropeanPerformance { position, places }` | The club at `position` qualifies for the CL when its association is one of the two best performers of the previous European season **among the associations the world simulates**, by the game's coefficient points per club |
| `Titleholder` | A competition's champion qualifies one competition up (EL champion → CL, ECL champion → EL, CL champion keeps its place). **Never the club an oversubscribed field cuts**; the club that is cut is offered the next competition down, so none is stranded |
| `Incumbent` | A club whose league the world does **not** simulate keeps its place: there is no domestic result to requalify it |

Modelled access list (APFM; shaped like UEFA's 2026/27 structure and chosen to reproduce the snapshot's
counts; **not** UEFA's allocation for any later season):

| Association | CL | CL by performance | EL | ECL |
|---|---|---|---|---|
| England | 1-4 | 5th | 5-6 + FA Cup winner | 7th |
| Spain | 1-4 | 5th | 5th + Copa winner | 6th |
| Germany | 1-4 | 5th | 5th + Pokal winner | 6th |
| Italy | 1-4 | 5th | 5th + Coppa winner | 6th |
| France | 1-3 | 4th | 4-5 + Coupe winner | 6th |

**Simulated:** domestic results of the five associations, European results (titleholders, performance
places), the cascades. **Synthetic (stated, not hidden):** the domestic leagues of the other 28
associations and UEFA's qualifying rounds are **not** simulated; their 70 clubs keep their places
(`Incumbent`) unless a European result moves them. A vacancy is refilled by the strongest unplaced club
of the European region **by reputation: the one place where reputation decides a place, and only for
vacancies**. Nothing here claims simulated UEFA qualifying rounds.

*Tested* (`ofm_core/tests/european_qualification_tests.rs`, six scenarios): domestic finishes and a cup
winner outside the league places; a titleholder moving up with the displaced and vacated places settled
and **asserted** (the cut club drops exactly one competition); one club with three routes holds one
place; incumbents keep their places; a newly promoted club qualifies by finishing in a place;
performance places go to the two best-performing associations and not to the worst. The authored
berths themselves are pinned in the adapter (`the_european_competitions_are_authored_with_their_qualification_berths`,
the `europe.berths` import check). **In the real world** the harness reads the 2027/28 fields against the
2026/27 archive: the Premier League's top four are in the Champions League, the CL and EL champions are
there, the ECL champion moved up, no club holds two places (`assert_next_fields_follow_the_season`).

## 8. Coefficients and ranking (Step 8)

Official 2026/27 club coefficients were **not obtainable** and none was invented. First edition: seeding
by club reputation; ranking criterion 10 uses the seeding order and the table reports
`coefficient_fallback_used`; the UI says so. From the second season: **APFM club coefficients computed
from the archived editions** (`league_phase::coefficient`): league-phase points 2/1/0, stage bonuses
1.5/1/0.5, five-season sum or 20 % of the association's, the association's season coefficient being its
clubs' points divided by the clubs it had in Europe. They seed the pots and feed criterion 10, are
reproducible from the game's own records (`field_from_teams_with` is tested), and are labelled as the
game's in the UI. **Not modelled:** knockout-match points, qualifying points, the revenue coefficient,
and the seasons before the career began.

## 9. Annex B and the neutral final (Step 9)

Read from the two Annex B diagrams:

* **Verified:** the R16 pairs seed pairs 1/2, 3/4, 5/6, 7/8 with the play-off winners of 15/16-17/18,
  13/14-19/20, 11/12-21/22, 9/10-23/24; a quarter-final joins the tie of seed pair 5/6 with 3/4, or 7/8 with
  1/2; each side's two quarter-final winners meet in its semi-final; the **silver** side's finalist is the
  nominal home team. `QUARTER_FINAL_PATHS` and `SEMI_FINAL_QUARTERS` agree with this
  (`the_layout_matches_the_annex_b_diagram`, written from the diagram, independent of the constants).
* **Not determinable from the diagram:** slots are labelled "5/6" (silver) and "6/5" (blue), i.e. which
  seed of a pair sits on which side is the draw's. The game fixes one arrangement (seeds 1, 3, 5, 8
  silver: UEFA's 2024/25 bracket). It is a valid reading, not the only one. **No discrepancy was found; nothing was changed.**
* **Neutral final (corrected).** The final's "home" team is a bracket label (19.05). The engine's home
  multiplier is removed for the final (`KnockoutLeg.neutral_venue` → `match_config_for`) and the nominal
  home side receives no gate. Domestic finals and every other fixture are unchanged. *Tested:* the
  leg flag, the engine configuration, the gate. *Not tested end to end:* an actual final played in the
  real world was not isolated; the helper's wiring in `create_live_match_with` is by reading. The
  spatial-match diagnostic (`spatial_adapter`) has its own fixed home factor and is not used for league play.

## 10. Calendars (Steps 3A and 10)

### Corrected profiles (Blocker A)

EUROPE4's "Thursday of the CL week" derivation was replaced by UEFA's published dates (table in section 3.1).
Exact-date tests: `the_europa_league_uses_the_published_2026_27_dates`, the Conference twin. Later seasons are
whole-week shifts of the template, never official.

### Domestic calendars planned around Europe

*Measured* on the real world, one PSG season, fixtures moved from the date they were generated for
(`CalendarStudy`; **the control column is the same world without the European competitions**, which
isolates cups and international windows from Europe):

| League | Control (no Europe) moved/max days | Before (EUROPE4 behaviour: sweep only) | After (planned around Europe) |
|---|---|---|---|
| Premier League (420) | 84 / 34 | 158 / 55 | 108 / 66 |
| Bundesliga (342) | 12 / 13 | 39 / 25 | 50 / 40 |
| Serie A (420) | 5 / 5 | 61 / 41 | 32 / 20 |
| LaLiga (420) | 6 / 5 | 91 / 25 | 90 / 50 |
| Ligue 1 (342) | 4 / 1 | 31 / 17 | 11 / 21 |
| Leagues with no European club (Championship, League One, Serie B, 2. Bundesliga, LaLiga 2, Ligue 2/3) | unchanged | unchanged | unchanged (the option is set on first divisions only) |

What changed: `MatchdayPattern.avoid_european_weeks` (serde default false; set on the first divisions):
the league's midweek rounds keep a day either side clear of every European matchday, and ordinary rounds
keep the 3-day rest window clear where their week has another weighted day, planned from the published
templates (`european_matchday_dates`). **No league is forced onto European weeks that none of its clubs
use** (leagues without the option are planned exactly as before: `a_league_can_keep_its_rounds_off_the_european_weeks`
with a positive control, and an old save loads with it off). **The effect is partial.** Across the five leagues that send clubs to Europe the number of moved fixtures fell from **380 to 291 (-23 %)** (the control world without Europe moves 111, so about 180 of the 291 are still Europe-caused), and Serie A (61 → 32) and Ligue 1 (31 → 11) improved most, but **the tail did not improve**: the longest delay is unchanged or worse in the Premier League (66 days against 55), the Bundesliga (40 against 25) and LaLiga (50 against 25), because the sweep that settles what planning cannot (a club with a league match, a cup tie and two European matches in one fortnight) still moves fixtures forward one at a time. This is recorded as a remaining defect (P1 for calendar quality), not as solved
Completion dates: every league still finishes within 8 days of its planned end. Matches per
month of PSG: July 2 · Aug 4 · Sep 5 · Oct 6 · Nov 6 · Dec 6 · **Jan 8** · Feb 4 · Mar 6 · Apr 5 · May 1 (the control career without Europe: 2/4/4/4/4/5/5/3/4/4). Peak load is eight matches in January; the 3-day rest rule held every day of every run.

## 11. The European match experience (Step 11)

There is **one** match engine and one result writer. PSG's Champions League matches were played through the
manager's own live workflow (`matchday::play_user_matchday_with_capture`) in
`psg_european_matches_play_through_the_managers_live_matchday`: eight league-phase matches, each with the
engine's report attached (what the 2D view and the post-match screen read), the table following the
results. Quick simulation, substitutions, tactics and injuries are the engine's own, not competition-specific:
`create_live_match_with` has no continental branch except the first-leg score and the neutral venue.
A European result changes only that competition (`competition_flow::after_result`); domestic tables are
untouched (the harness's invariants held for every day of every run). **Not operated:** the interactive 2D
pitch and the pre-match screens in the real app (section 16).

## 12. Manager-facing information (Step 12)

* **Inbox/news** (`league_phase::announce`, once each through the sent-ledger, tested for repeats and reload):
  the draw (first opponent and date), the league-phase result (three variants), each knockout tie
  (won/lost with the score from the club's side), the title, and a front-page story for every edition.
  The final is told as the title only. The competition is named in the player's language
  (`competitionKey` → `competitionDisplayName`, tested in French).
* **Tournaments:** the club's position and the play-off cut-off while the table is open, the draw/seeding
  basis, pot trades, the prize-assumption note, the neutral final marker, the existing table, bracket and
  failed-draw state.
* **Calendar and Dashboard** read the same fixtures every competition feeds (a European fixture is a
  fixture of its competition); no second source of competition state was added. A dedicated dashboard card
  was **not** built.
* **i18n:** 30 keys in all 12 locales; reviewed by `i18n-auditor` (findings applied). **Accessibility**
  reviewed by `ui-accessibility-reviewer` (findings applied). **Not verified:** light/dark rendering, window
  sizes, keyboard navigation and contrast in a browser or in the Tauri app: none was available.

## 13. Prize money and finances (Step 13)

Amounts are **game-balance assumptions** (`balance_prize`, labelled in the UI and the docs), not UEFA's
distribution (none was retrievable). Once-only posting is keyed per payment and stored with the
competition; the harness checks, with an oracle derived from the fixtures alone (not from the prize
module), that for every archived edition the posted payments are unique and **exactly** as many as the
results earned (participation + wins + draws + each club in each knockout tie + the champion), so a retry,
a reload or the rollover paid nothing twice and no club was paid for a match it did not play or a round it
did not reach (`assert_prize_money_posted_once`, 2026 and 2027). Budgets, cash and wages were checked by the
harness invariants for every day. The neutral final pays no gate to its nominal home side.

## 14. Persistence and installation safety (Step 14)

* The same input and configuration give an **identical world file** (`--verify-determinism`: two independent
  runs, same SHA-256). A failed import writes nothing (a blocking check refuses the world).
* Existing careers keep loading: `LeaguePhaseState.pot_adjustments`, `MatchdayPattern.avoid_european_weeks`
  and the new `BerthRule` variants are inside existing JSON columns with `serde(default)` and
  `skip_serializing_if`; no SQL migration, no `MIGRATION_COUNT` change. An old career without European
  competitions is neither overwritten nor given a historical European season
  (`pending_editions_are_drawn_once_and_old_careers_are_left_alone`; the adapter's `european = false`).
* `a_mid_season_european_career_survives_the_sqlite_save`: a PSG career at the knockout stage written to a
  SQLite save and loaded from a fresh manager: every European competition identical, the same squads, and 30
  further days played to the same fingerprint.
* Between stages: `three_european_seasons_survive_a_reload_at_every_stage` (JSON round trip at every stage,
  between the legs, before a final, and immediately before and after each rollover).
* An unfinished edition survives the rollover and is renewed only when it and its siblings are finished;
  an edition is archived exactly once and the next generated exactly once (`assert_editions_archived`).
* Stable identities: `fm-c-*`/`fm-p-*` from the source's internal ids, `eu-gen-ge-iberia-tbilisi` for the
  generated club.

## 15. Performance (Step 16)

Measured on this Mac (Apple silicon, dev profile with optimisation as the repository's test profile),
one PSG career, the same seed, the **real FM world** built twice from the same source: with and without
the European competitions (`--no-european`). `what_a_career_costs` (an ignored measurement test) prints
the numbers; the machine was otherwise idle for these runs.

| | Control (no Europe) | With Europe | Change |
|---|---|---|---|
| Clubs / players in the world | 240 / 13,358 | 310 / 16,609 | +70 clubs (+29 %), +3,251 players (+24 %) |
| World build (`apfm-import`, from `world.db`) | 2.1 s | 2.4 s | +0.3 s |
| Career start (`build_fm24_career`: game build, history, scope) | 39.7 s | 39.8 s | unchanged |
| One full season (day loop to the rollover) | 25.5 s | 41.7 s | **+16 s (+63 %)** |
| SQLite save / load | 0.36 s / 0.30 s | 0.46 s / 0.36 s | +0.1 s / +0.06 s |
| Save file size | 37.5 MB | 47.1 MB | +9.6 MB (+26 %) |
| Peak resident memory (whole test process) | 679 MB | 829 MB | +150 MB (+22 %) |
| Three seasons with a reload at every stage (+75 reloads) | n/a | 229-265 s per seed (seeds 3, 7, 11) | |

* **440-club generated world** (the shipped Random World, regression baseline, `apfm_perf native`): 14,341
  players, mean 204 ms a day over 303 days (p95 515 ms), save 51.7 MB in 0.51 s, load 0.34 s, peak 702 MB.
  The European FM world (310 clubs) costs a season about the same as that world per club, and its save is
  smaller than the generated one.
* **The cost of a non-European career is not increased by competitions it is not in** (a club's own
  competition decides what the engine plays), but **it is increased by the 70 extra clubs** (the +63 % per
  season, mostly the daily AI loops for 70 more teams and the 3 x 36-club competitions settled by scoreline
  in the dormant tier). If a lighter world is wanted, the option is to cap the continental-only clubs'
  daily AI work; it was not done here.
* **European fixture cost:** 144 + 144 + 108 league-phase and 45 x 3 knockout fixtures a season, drawn once
  an edition. Draw plus schedule is sub-second in the 900-draw study (worst case 1.5 s in EUROPE4).
* **Calendar reconciliation:** the daily sweep is unchanged; planning around Europe adds one
  `european_matchday_dates` computation per league per season (three calendar templates). No quadratic
  daily search was introduced.


## 16. Verification evidence, reviewers and honesty about what was not done

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean (both) |
| `cargo test --locked --workspace --no-fail-fast`, default | **5,622 passed, 0 failed**, 156 ignored (5,566 at the baseline) |
| same with `--features mcp` | **5,709 passed, 0 failed**, 156 ignored (5,653 at the baseline) |
| Vitest (whole frontend) | **3,309 passed**, 4 skipped, 0 failed (3,302 at the baseline) |
| `tsc` + `vite build` | passes |
| Biome format and lint (Biome 2.5.5, the installed one, not `npx`) | clean (3 pre-existing infos) |
| Knip | clean (2 pre-existing configuration hints) |
| i18n gates (`localeCoverage`, `frontendKeyCoverage`, placeholder parity) | pass; 30 new keys in 12 locales |
| fm24-adapter tests that need the real `world.db` (`APFM_WORLD_DB`, `--ignored`) | **13 of 13 pass** (`data2_named_players` 10, `realism3_value_scale` 3) with the European mapping on, after the registry and 309-club fixes below. `real_world_hierarchy`'s one test needs a rating-pairs file (`APFM_RATING_PAIRS`) and was **not run** |
| Real-world harness (`APFM_WORLD_JSON` = a world built in scratch), `integration5_europe`, 7 tests + 1 measurement | **all pass** (details below) |
| Older real-world harness suites on the European world (`fm24_*`) | career 2/2, competitions 2/2, ratings 3/3, loans 4/4 pass; **3 failures**, all explained below |
| Mutation checks | **23 of 23 applied mutations killed, 0 survived** (list below) |

**Real-world acceptance (the owner's FM data, built in scratch, never installed)**

* `a_new_psg_career_opens_with_the_champions_league_drawn`: three fields of 36, disjoint, drawn and dated at
  career start, PSG in the CL with eight fixtures, no rest-window clash.
* `the_first_european_season_is_played_to_its_finals_and_archived`: a whole PSG season through the
  production day loop and rollover, finals on UEFA's published dates (EL 26 May, ECL 2 Jun, CL 5 Jun 2027),
  every invariant clean, each edition archived once with its champion, prize money posted once, and the
  2027/28 fields following the 2026/27 results.
* `psg_european_matches_play_through_the_managers_live_matchday`: PSG's eight Champions League matches
  played through the manager's own live workflow, each with the engine's report.
* `each_kind_of_club_plays_exactly_its_own_european_season`: **PSG (CL), Marseille (EL), Monaco (ECL), a
  Ligue 1 club with no European football**: each in exactly its competition (none for the last), each
  follows only that competition, each plays its matches there, no extra European competition appears.
* `three_european_seasons_survive_a_reload_at_every_stage`: **three seasons x three seeds (3, 7, 11)**, a
  save/reload at every stage change (75 per run: between the legs of a tie, before a final, before and
  after every rollover), every competition byte-identical after each reload, three editions archived once
  each, prize money once, next fields following the archive, no club in two competitions.
* `a_mid_season_european_career_survives_the_sqlite_save`: a real SQLite save of a PSG career at the
  knockout stage and a load from a fresh manager: identical competitions, 30 further days to the same fingerprint.
* A synthetic European season (EUROPE4's harness) **still passes** (it ran in the workspace suite).
* **Determinism:** the world file is byte-identical across independent builds (`--verify-determinism`).
* **No test above counted as a real-source verification unless it printed no `SKIPPED` line**: every
  real-world test now prints `SKIPPED: APFM_WORLD_JSON is not set` when it returns early.

**Defects the real world found during this mission (all fixed, each with a red-first regression test)**

1. The European competitions were authored **without their qualification berths** (a silent no-op of my own
   edit), so the incumbent/titleholder rules were inert in the shipped world. Fixed; pinned by an adapter
   test and an import check (`europe.berths`).
2. **Two competitions short of clubs refilled from the same pool and picked the same club**, leaving it in
   two European fields (predated this mission; made reachable by routine vacancies).
3. A finished edition could be renewed at the rollover while a sibling was still being played, losing the
   sibling's titleholder berth. Fixed and shared with the harness invariant.
4. The registry listed the players of the 69 added clubs as **external** players (a second owner) and fitted
   its valuation on a different population from the world's. Fixed.
5. Imported worlds were never drawn (competitions were shells): `draw_pending_editions`.
6. The inbox/news named the competition in English in every locale: now translated from its key.
7. The final was announced both as a won tie and as the title.

**Failures in the older suites on the European world, and why they are not this mission's**

* `fm24_free_agents::the_player_can_sign_an_fm24_free_agent_at_the_wage_he_asks` ("squadFull count=23 max=30")
  and `fm24_global_market::clubs_outside_the_players_league_buy_players` (72 transfers where it expects 120):
  **both fail identically on the control world without Europe** (48 transfers there), and no file this
  mission touched is in the free-agent, contract or transfer code. They were **not** run at the baseline
  commit (no second build was made, disk), so "pre-existing" is established by the control, not by the baseline.
* `fm24_rosters::a_player_registered_elsewhere_is_on_loan_from_his_owner` expected Pafos to be outside the
  world; Pafos is now a world club (Conference League), so the test expectation was updated (the loan
  names no outside club).

**Mutation checks** (a rule broken in production code, the named test run, expected red, the file restored
byte for byte): titleholder protected from the cut; cut clubs offered the next competition; incumbents keep
places; performance places; pot relaxation (function and its use in the edition); neutral final multiplier;
neutral final gate; EL and ECL published dates; draw announcement once; European-only nations in the pyramid
rule; ordinary rounds' rest window around Europe; coefficient window; draw points; zero association limit;
pending editions drawn; renewal waits for a sibling; the final announced once; a filler placed for the next
field; the adapter refuses a renamed source club; refuses a club in two fields; authors the European berths.
**23 killed, 0 survived.** (Two adapter mutations first failed to compile and were redone in compilable form.)

**Reviewers** (all read-only, run on the committed tree)

| Reviewer | Result | Disposition |
|---|---|---|
| `ofm-architecture-reviewer` | P1: rollover renewed a finished edition while a sibling was played. P2: neutral final only on the live path; positional qualifiers cut by reputation; clone per day; magic priority; narrow-window fallback; docs; size caps | P1 fixed; berth docs fixed; announce clone reduced; size caps fixed (`pipeline.rs` 1,605 -> 1,318, `resolve_berth_fields`, `european_checks`); others recorded in section 17 |
| `ofm-test-reviewer` | P0: berths not authored (fixed). P1: vacuous without the world (now printed), helper-only neutral test, ES access list, cascade not asserted, orchestration untested, coefficient seeding untested, announce orientation, flag with no positive control, validation untested | All applied except the wiring test of `create_live_match_with` for a final (recorded) |
| `ofm-dedup-reviewer` | P1: berth helper had no caller (same defect as P0); association/coefficient history duplicated. P2: pot-block count twice, fixtures copied, reload loop, `params`, constant, profile mapping | Both P1 fixed, `FIELD_SIZE` shared, dead binding removed; the rest named for epic #589 in section 17 |
| `i18n-auditor` | P1: English competition name in every locale; Russian cases. P2: es articles/register, tr "Aferin", cs/de/it/pt/fr wording, bracket note false when no shootout, 36-club wording | P1 and the P2 translations fixed; bracket note removed; "UEFA" wording on every league phase recorded |
| `ui-accessibility-reviewer` | No P1. P2: neutral marker duplicated/split, notes list name, concatenated sentences | Applied (notes in one list, marker `aria-hidden` + sr-only, two spans); contrast **not measured** (no browser) |


## 17. Remaining defects and limits

No known P0. Everything below is stated, not hidden.

| # | Sev | Item | Evidence / consequence |
|---|---|---|---|
| 1 | **P1** | **The GUI was not operated.** No display or assistive technology was available; the real Tauri app, the 2D match, the light/dark themes, window sizes, keyboard navigation and contrast were not checked. Vitest and the harness are not app acceptance | The owner's alpha test is the first real acceptance |
| 2 | **P1** | **Calendar quality is only partly solved**: moved fixtures fell 23 % but the worst delays (50-66 days in LaLiga and the Premier League) did not improve | Section 10; a per-club European-aware scheduler (not a sweep) is the next step |
| 3 | P2 | The access list is **APFM-modelled**; UEFA's allocation for later seasons, qualifying rounds and the real coefficient ranking are not modelled | Section 7 |
| 4 | P2 | **28 associations' domestic leagues are not simulated**; their 70 clubs keep their places unless a European result moves them. A vacancy is filled by reputation | Section 7 |
| 5 | P2 | Pots by **club reputation** in the first edition and by the game's own coefficients afterwards (official coefficients were not retrievable); the draw is the game's own, not UEFA's | Sections 3, 8; all labelled in the UI |
| 6 | P2 | Annex B: which seed of a pair sits on which side is fixed (the draw's in reality) | Section 9 |
| 7 | P2 | Neutral final: the engine and the gate are corrected; the spatial-match diagnostic keeps its own home factor; a final played in the real world was not isolated; the create-match wiring has no test of its own | Section 9 |
| 8 | P2 | Prize amounts are **game-balance assumptions** | Section 13 |
| 9 | P2 | Overflow: when a field is oversubscribed, titleholders are protected but a positional qualifier can be cut by reputation (offered the next competition down) | `cascade_fields` |
| 10 | P2 | Source quirks carried into the world: 4 clubs without a head coach (generated manager), Iberia Tbilisi generated, Lincoln Red Imps without a nation (association from UEFA), CSKA Sofia medium confidence; 103 info-level world-audit items (duplicate person Duje Matić, squads up to 65 players, a 13-year-old) | `ADAPTER_REPORT.md` of the scratch world |
| 11 | P2 | Coefficient bonus keyed on competition priority (1 / 2 / other), European dates read from the shipped profiles (a modded calendar is ignored), planner fallback when a window is too narrow | `coefficient.rs`, `schedule_pattern.rs` |
| 12 | P2 | The frontend marks the final neutral by `stage === "Final"` (a backend rule); "UEFA" wording is shown for every league-phase competition; messages say "two matches" / "the best-placed clubs" for the 36-club format | Modded league phases would read slightly wrong |
| 13 | P2 | Dedup debt named for epic #589: pot-block count in `pots.rs` and `draw.rs`; copied test fixtures; a `tests/support` module; the reload-and-compare loop; `params`/`league_office_message`/`news::emit_once`; `profiles::for_short`; `domestic_leagues(game)` | Dedup reviewer |
| 14 | P2 | Size: `pipeline::run` (about 440 lines), `checks::run_checks`, `end_of_season/mod.rs` (1,555), `finances/mod.rs` (1,567) were past the caps before this mission and grew by a few lines; the three limits this mission crossed were fixed | Architecture reviewer |
| 15 | P2 | The older suites `fm24_free_agents` and `fm24_global_market` fail on the control world too (section 16); the baseline commit was not rebuilt to confirm | Section 16 |
| 16 | P2 | Real-world tests need the owner's file; **without `APFM_WORLD_JSON` they print SKIPPED and verify nothing** | Section 16 |


## 18. Installation procedure for the owner's acceptance test

Nothing was installed. The world is built **to a scratch directory**; the installed world is not touched.

```bash
cd <repo>                                       # at the final SHA
cargo run --release -p fm24-adapter --bin apfm-import -- \
    ~/apfootballmanager/data/world.db  ~/apfm-scratch/europe-world        # read-only on world.db
shasum -a 256 ~/apfootballmanager/data/world.db                            # must equal section 2
# the world file: ~/apfm-scratch/europe-world/apfm-fm24-2026.json
```

To try it without replacing anything: run the app against a **copy** of its data directory (or a fresh
profile), install `apfm-fm24-2026.json` there as the world, start a new PSG career, and check in the
Tournaments tab that the Champions League is drawn and PSG has eight league-phase fixtures. Existing saves
are unaffected; a new career is required to see the European competitions. **To revert**, delete the
scratch directory (and the profile copy): the original world and saves were never modified. The importer
refuses to write if any blocking check fails, and `--no-european` rebuilds the previous five-country world.
Do not point the importer's output at the installed world directory.
