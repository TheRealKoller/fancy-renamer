# AGENTS.md – Arbeitsanweisung für KI-Agenten

Dieses Projekt wird vollständig von KI-Agenten entwickelt. Der Mensch (Owner: @TheRealKoller)
schreibt Issues, beantwortet Rückfragen und testet das Produkt als Ganzes. Alles andere –
Refinement, Tests, Implementierung, PR, Merge – liegt bei dir.

## Projekt

- **fancy-renamer**: Datei-Umbenennungstool mit grafischer Oberfläche für **Fedora Linux**.
- Repo: `TheRealKoller/fancy-renamer`, Lizenz GPL-3.0.
- Sprache für Issues, Kommentare, Commits, PRs und Doku: **Deutsch**. Code-Bezeichner: Englisch.
- Tech-Stack, Build- und Testbefehle: siehe Abschnitt [Stack & Befehle](#stack--befehle).

## Issue-Lebenszyklus (Labels)

| Label | Bedeutung |
|---|---|
| `needs-spec` | Noch nicht refined: Akzeptanzkriterien fehlen oder sind unklar |
| `needs-info` | Offene Rückfrage an den Owner, wartet auf Antwort |
| `ready` | Refined, Akzeptanzkriterien vollständig und prüfbar → umsetzbar |
| `in-progress` | Ein Agent arbeitet gerade daran (verhindert Doppelarbeit) |
| `prio:high` / `prio:medium` / `prio:low` | Priorität (ohne Label = medium) |
| `feature` / `bug` / `ui` / `infra` / `documentation` | Art der Arbeit |

Immer genau **ein** Status-Label (`needs-spec`, `needs-info`, `ready`, `in-progress`) pro offenem Issue.

## Ablauf einer Session

### 1. Nächstes Issue wählen

```sh
gh issue list --state open --json number,title,labels,updatedAt --limit 100
```

Reihenfolge:
1. Issues mit `needs-info`, auf die der Owner inzwischen geantwortet hat (neuer Kommentar des Owners nach deiner Frage) → weiter refinen.
2. `ready`-Issues: höchste Priorität zuerst, dann `bug` vor `feature`, dann niedrigste Nummer. Abhängigkeiten („blockiert durch #N“) beachten.
3. Wenn nichts `ready` ist: `needs-spec`-Issues refinen (gleiche Prioritätsreihenfolge).
4. Issues mit `in-progress` nicht anfassen, außer der Branch ist verwaist (> 24 h keine Aktivität) – dann übernehmen und das im Issue vermerken.

Teile dem Owner in der Session kurz mit, welches Issue du nimmst und warum.

### 2. Refinement (bei `needs-spec` / `needs-info`)

Ziel: Jedes Issue hat am Ende eine klare User Story, **prüfbare Akzeptanzkriterien** (Checkliste) und Nicht-Ziele.

1. Issue, Kommentare und betroffenen Code lesen. Alles, was sich aus Code, Doku oder bestehenden Konventionen beantworten lässt, **selbst** entscheiden – nicht fragen.
2. Echte Unklarheiten (fachliche Entscheidungen, UX-Präferenzen, Zielkonflikte) sammeln und **gebündelt** stellen, jeweils mit Optionen und deiner Empfehlung.
3. **Zuerst in der Session fragen.** Kommt keine Antwort oder ist klar, dass der Owner nicht da ist (z. B. autonomer Lauf, explizite Ansage), die Fragen **als Kommentar am Issue** stellen, Label `needs-info` setzen und mit dem nächsten Issue weitermachen.
4. **Jede Klärung muss im Issue stehen.** Antwortet der Owner im Chat, fasse Frage + Antwort als Kommentar im Issue zusammen („Klärung aus Session vom JJJJ-MM-TT: …“).
5. Den Issue-Body aktualisieren (Abschnitte *Ziel*, *Akzeptanzkriterien*, *Nicht-Ziele*, *Technische Notizen*), damit der Body allein ausreicht. Dann Label → `ready`.
6. Zu große Issues in Teil-Issues aufteilen (`gh issue create`, im Eltern-Issue verlinken).

### 3. Implementierung (bei `ready`)

1. Label `ready` → `in-progress`, kurzer Kommentar „Starte Umsetzung“.
2. Branch von aktuellem `main`: `<typ>/<issue-nr>-<kurzbeschreibung>`, z. B. `feat/12-regex-umbenennung`, `fix/15-crash-leerer-ordner`.
3. **Test zuerst**: Für jedes Akzeptanzkriterium mindestens einen Test, der vorher fehlschlägt. UI-Logik so schneiden, dass die Kernlogik ohne UI testbar ist.
4. Implementieren, bis alle Tests, Lint und Typprüfung lokal grün sind.
5. Kleine, thematische Commits im Format [Conventional Commits](https://www.conventionalcommits.org/) (`feat: …`, `fix: …`, `test: …`, `docs: …`, `chore: …`), Text auf Deutsch.
6. Weicht die Umsetzung vom Issue ab oder tauchen neue Fragen auf: wie beim Refinement fragen und im Issue dokumentieren. Neue, nicht zum Scope gehörende Erkenntnisse als **neues Issue** (`needs-spec`) anlegen, nicht nebenbei umsetzen.

### 4. Pull Request & Merge

1. PR gegen `main` mit dem PR-Template; Body enthält `Closes #<nr>` und pro Akzeptanzkriterium, wie es nachgewiesen ist (Test-Name oder manueller Check).
2. Selbst-Review des Diffs: toter Code, Debug-Ausgaben, fehlende Tests, Doku/CHANGELOG.
3. Warten bis alle CI-Checks grün sind (`gh pr checks --watch`). Bei Rot: fixen, nicht umgehen.
4. Merge selbst durchführen: `gh pr merge --squash --delete-branch`.
5. **Ausnahme – Owner-Review anfordern** statt selbst zu mergen (Label `needs-review`, Owner im PR erwähnen), wenn:
   - Sicherheitsrelevantes (Dateisystemzugriffe außerhalb der vom Nutzer gewählten Dateien, Löschen, Rechte, Netzwerk),
   - Änderungen an CI, Branch-Schutz, Release- oder Packaging-Prozess,
   - irreversible Datenänderungen beim Nutzer oder Breaking Changes,
   - du dir bei einer Designentscheidung trotz Refinement unsicher bist.
6. Nach dem Merge: Issue ist via `Closes` geschlossen; kurzer Abschluss-Kommentar, falls etwas für den Gesamttest des Owners relevant ist („Zum Testen: …“).

## Leitplanken

- **Nie** direkt auf `main` pushen, nie Force-Push auf `main`, Branch-Schutz nie ändern ohne Owner-Freigabe.
- Ein Issue pro Branch/PR. Kein Scope-Creep.
- Das Tool benennt Nutzerdateien um: **Datenverlust ist der schlimmste Fehler.** Jede Umbenennungslogik braucht Tests für Kollisionen, Sonderzeichen, Groß/Kleinschreibung, lange Pfade und Abbruch mittendrin; Vorschau vor Ausführung und Undo sind Kernprinzipien.
- Keine Secrets ins Repo. Neue Abhängigkeiten nur, wenn nötig und in Fedora gut verfügbar; im PR begründen.
- Fehlschlagende Tests nie löschen oder abschwächen, um grün zu werden.

## Stack & Befehle

> Noch nicht festgelegt – wird im Rahmen der Anforderungsphase entschieden und hier eingetragen
> (Sprache, UI-Toolkit, Build, Test, Lint, lokaler Start, Packaging).
