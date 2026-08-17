<!-- Verified research reference, gathered 2026-08-13 by a fan-out of thirteen agents and an
     adversarial verification pass over every claim. Sources are cited inline; the markers at
     the top say how firmly each fact is held. Written because the person building this overlay
     is blind and cannot inspect Melodyne's UI, so every fact here is a screenshot he does not
     have to capture and a round of testing he does not have to run.

     Un-wrapped 2026-08-17. This file used to BE the workflow's raw JSON output, with the whole
     document sitting inside a "result" string — unreadable as prose, unrenderable by the docs
     site, and not even parseable as JSON: the payload contains unescaped double quotes, so
     every parser stops at the first of them, 2115 characters in. The text below is that string
     decoded, unchanged apart from the corrections marked [KAL], which are dated and say what
     they replace. The workflow's own metadata (token counts, agent bookkeeping) is gone; it
     said nothing about Melodyne. -->

# Melodyne 5 studio — Referenz für das Overlay
**Zusammenführung von fünf verifizierten Rechercheberichten · Stand der Prüfung: 13.08.2026**

---

## 0. Wie dieses Dokument zu lesen ist

**Markierungen** (stehen direkt am Fakt, nicht am Absatz):

| Marke | Bedeutung |
|---|---|
| **[Q]** | In der zitierten Quelle wörtlich nachgeprüft |
| **[BIN]** | Aus der installierten DLL extrahiert — *Evidenz, keine Celemony-Dokumentation* |
| **[FIG]** | Aus einer Celemony-Abbildung gemessen/angesehen — nicht im Fließtext dokumentiert |
| **[INF]** | Schlussfolgerung, von keiner Quelle so gesagt |
| **[FORUM]** | Forum/Wiki/Blog — niedrigere Autorität, ausdrücklich als solche gekennzeichnet |
| **[LÜCKE]** | Nirgends gefunden — steht in Teil 2 als konkrete Frage |
| **[≠]** | Quellen widersprechen sich — beide Seiten genannt |
| **[KAL]** | Auf dieser Maschine aus einer Kalibrieraufnahme gemessen — echtes Melodyne unter Windows |
| **[LOG]** | Aus dem Log dieser Plattform gemessen, während jemand Melodyne wirklich benutzt hat |

**Grundwarnung zu allen Pixelangaben:** Sämtliche Abbildungen von Celemony sind **macOS-Screenshots** [FIG]. Es gibt in keiner autoritativen Quelle ein Windows-Bild von Melodyne 5. Alle Pixelmaße unten sind **Verhältnisse innerhalb der Abbildung**, keine bestätigten nativen Windows-Pixel. Drei Quellen liefern drei Skalen (Celemony ~31 px Buttonraster @1×, protoolstraining 26 px, altes Melodyne 22 px) — **keine davon ist Grundwahrheit für die Zielmaschine.**

---

# TEIL 1 — WAS GESICHERT IST

## 1. Version, Dateien, Prozess

**[Q/BIN]** Installiert auf dieser Maschine:
- `C:\Program Files\Celemony\Melodyne 5\Melodyne.exe` — `FileVersion`/`ProductVersion` = **5, 4, 1, 4**; `ProductName=Melodyne`, `CompanyName=Celemony Software GmbH`
- `C:\Program Files\Common Files\Celemony\Bundles\MelodyneCore-5.4.1.004.dll` (89.558.568 B, 10.07.2024) — daneben `-5.4.0.036`, `-5.3.1.018`
- `C:\Program Files\Common Files\VST3\Celemony\Melodyne\Melodyne.vst3`
- `C:\Program Files\Common Files\Avid\Audio\Plug-Ins\Melodyne.aaxplugin\Contents\x64\Melodyne.aaxplugin`

**[Q]** Das Online-Handbuch dokumentiert bereits **5.4.2** (Versionsgeschichte, PDF S. 272) — die Maschine ist einen Patch zurück. Was 5.4.2 geändert hat, ist hier ungetestet.

**Handbücher** (beide „Last updated on 07/01/2026"):
- Stand-alone, 280 S.: `https://helpcenter.celemony.com/M5/pdf/melodyneStudio5/en?env=standAlone`
- ARA, 234 S.: `https://helpcenter.celemony.com/M5/pdf/melodyneStudio5/en?env=dawsWithAra`
- Seitenzahl im Fußtext = PDF-Index, „S. N" unten ist also direkt prüfbar. **[Q]** Das PDF wird aus dem Help Center generiert → **immer zusätzlich über die Überschrift zitieren**, Seitenzahlen driften.
- **[Q]** Das Help Center enthält Seiten, die in **keinem** PDF stehen (z. B. `M5tour_TransferARA`). PDF ⊂ Help Center.
- Seitenindex: `https://helpcenter.celemony.com/M5/tocs.json` (88 `M5tour_*`-IDs über alle Editionen). `env=transferPlugin` → HTTP 500, es gibt nur `standAlone` und `dawsWithAra` als PDF.

## 2. Fenster, Fensterklassen, Win32-Oberfläche

### 2.1 Fensterklassen [BIN]

Scan nach UTF-16LE-Strings `GN…Window…` in `MelodyneCore-5.4.1.004.dll` liefert **genau drei**, zusammenhängend:

```
0x2FD47E8  GNWindowMenu
0x2FD4808  GNWindow
0x2FD4820  GNWindowDoc
```

Dieselbe Trias in 5.3.1.018 (`0x2EE8A50/70/88`) und 5.4.0.036 (`0x2F9BFD8/FF8/0x2F9C010`) → **stabil 5.3.1 → 5.4.1**. `RegisterClassW` und `CreateWindowExW` sind importiert (kein `RegisterClassExW`), passend zu Wide-Klassennamen.

**[BIN]** Zusätzlich, in einem UTF-16-Cluster mit `Window`, `Edit` und `tooltips_class32`: **`GNEmbedded%p`** und `GNDragImage%p`. **[INF]** Die eingebettete/Plug-in-Kindklasse wird pro Instanz mit Zeigersuffix erzeugt → **niemals auf den vollen String matchen, immer auf das Präfix `GNEmbedded`.**

**[INF, ungeprüft]** Dass `GNWindowDoc` das Dokument-/Hauptfenster ist, `GNWindow` ein Hilfsfenster und `GNWindowMenu` ein selbstgezeichnetes Menüfenster, ist reine Namens- und Nachbarschaftsschlussfolgerung. **Kein Live-Probe wurde gefahren.**

**[Q/BIN]** `Melodyne.exe` und `Melodyne.vst3` enthalten **keine** `GNWindow*`-Strings, aber alle drei Binaries enthalten den Formatstring `\Celemony\Bundles\%hsCore%hs` → das Fenster wird vom gemeinsamen Core erzeugt. Das AAX-Binary enthält ASCII-`GNWindow`-Fragmente (statisch gelinkter GN-Code), aber **keine** UTF-16-Klassennamen — die Registrierungsaussage bleibt unberührt.

**[LÜCKE]** Der **Fenstertitel** ist nirgends dokumentiert. `SetWindowTextW` ist importiert, Titel werden also zur Laufzeit gesetzt. → **Match auf `Melodyne.exe` + Klasse, nie auf den Titel.**

### 2.2 Keine Accessibility-API — der wichtigste Einzelbefund [BIN]

Vollständiger PE-Import-Parse (38 importierte DLLs, **Delay-Load-Verzeichnis-RVA = 0**, die Importliste ist also komplett) plus String-Scan über 89,5 MB, ASCII und UTF-16, case-insensitiv:

**Null Treffer** für `oleacc`, `UIAutomationCore`, `IAccessible`, `LresultFromObject`, `ObjectFromLresult`, `CreateStdAccessibleObject`, `AccessibleObjectFromWindow`, `NotifyWinEvent`, `UiaReturnRawElementProvider`, `IRawElementProviderSimple`, `WM_GETOBJECT`.

(Die einzigen `accessib*`-Treffer sind das Feld `isAudioAccessible` und `NSAccessibility*`-Schlüssel in einem eingebetteten **macOS-NIB-Archiv** — kein Windows-Code.)

→ **Melodyne implementiert unter Windows weder einen MSAA-Server noch einen UIA-Provider.** Was ein Screenreader sieht, kommt aus `DefWindowProcW` plus der Eigen-Accessibility von USER32 (Menüleiste) und COMDLG32 (Dateidialoge). **Für die Notenfläche gibt es keinen Weg außer Pixel/OCR.** Das ist auch der Grund, warum SIBIAC für Melodyne OCR benutzt, für andere Plug-ins aber „NVDA text" [FORUM, Changelog 13.11.18].

### 2.3 Menüs: Menüleiste nativ, Kontextmenüs nicht [BIN + FORUM]

**Importiert aus USER32:** `CreateMenu`, `SetMenu`, `GetMenu`, `GetSystemMenu`, `DrawMenuBar`, `DestroyMenu`, `GetMenuItemCount`, `RemoveMenu`, `DeleteMenu`, `CheckMenuItem`, `EnableMenuItem`, `GetMenuInfo`, `SetMenuInfo`, `InsertMenuItemW`, `GetMenuItemInfoW`, `SetMenuItemInfoW`.

**Nicht importiert:** `CreatePopupMenu`, `TrackPopupMenu`, `TrackPopupMenuEx`.

**[FORUM]** SIBIAC-Autor azslow3 zu Melodyne 4: *„Melodyne use Alt key for special actions, so you can not get there usual way. **But menu itself is standard.**"* (azslow.com Topic 427)

→ **Verwertbar:** Die Menüleiste lässt sich mit `GetMenu` → `GetSubMenu` → `GetMenuItemInfoW` aufzählen und mit `WM_COMMAND` + Item-ID auslösen — **ohne Alt, ohne F10, ohne SC_KEYMENU**. (Melodyne importiert `GetSubMenu` selbst nicht; das Overlay ruft es auf, das ist unbetroffen.)
→ **Nicht verwertbar:** Rechtsklick-Kontextmenüs, die Grid-Popups an Notenschlüssel-/Notenwert-Icon, das Scale-Ruler-Menü, das Algorithmus-Popup im Inspector. Ressource `MDContextMenues.gnui` [sic] — selbstgezeichnet, plausibel in `GNWindowMenu`-Fenstern. Pixel/OCR.

### 2.4 Tastatur-Pfad und DPI [BIN]

- **Accelerator-Tabelle:** `CreateAcceleratorTableW`, `TranslateAcceleratorW`, `DestroyAcceleratorTable`, eigene Message-Pumpe (`GetMessageW`, `PeekMessageW`, `TranslateMessage`, `DispatchMessageW`), eigene Tastendekodierung (`GetKeyboardState`, `ToUnicode`, `ToAscii`, `MapVirtualKeyW`), `GetAsyncKeyState` für Live-Modifier.
  **[INF, zuerst testen]** `TranslateAcceleratorW` sieht nur Nachrichten aus der eigenen `GetMessage`-Schleife → **`PostMessage(WM_KEYDOWN)` kann Shortcuts auslösen, `SendMessage(WM_KEYDOWN)` umgeht den Accelerator-Pfad.** Beide, `SendMessageW` und `PostMessageW`, sind importiert.
- **Kein `RegisterHotKey`, kein `SetWindowsHookExW/A`, kein `AttachThreadInput`** → Melodyne beansprucht keine systemweiten Tasten und kämpft nicht mit den Hotkeys des Overlays.
- **DPI:** Wide-String `L"Shcore.dll"` (`0x2FE7830`) + `GetDpiForMonitor` (`0x2FE7848`), `SetProcessDpiAwareness`, `GetProcessDpiAwareness`. **Nicht vorhanden:** `SetProcessDpiAwarenessContext`, `GetDpiForWindow`, `AdjustWindowRectExForDpi`, `SetThreadDpiAwarenessContext`. Nur das alte `AdjustWindowRectEx`.
  **[INF]** Altes Shcore-Modell (System- oder Per-Monitor-v1) → **ein bei 100 % erfasstes Layout passt bei 125 %/150 % nicht. DPI mit jedem Koordinatensatz mitspeichern.**
  **[FORUM]** SIBIAC verlangt hart 100 %: *„Sibiac will fail to work in case that is not checked."*
- **`UpdateLayeredWindow`** ist importiert — einige Melodyne-Flächen sind Layered Windows. Relevant für die Wahl der Capture-Methode.
- **[BIN]** Tastennamen-Vokabular = 21 Einträge `GNEvent.StringTable.*`: ` (Num)`, `Alt-`, `Backspace`, `Command-`, `Control-`, `Delete`, `Down`, `Escape`, `Form Feed (next page)`, `Help`, `Insert`, `Insert.2`, `Left`, `Line Feed`, `Return`, `Right`, `Shift-`, `Space Bar`, `Tab`, `Up`, `Vertical Tab (prev page)`. `F%i` existiert nur als **Formatstring**. Es gibt **kein** `Home`, `End`, `Page Up/Down`, `Clear`, `fn`, `Num Lock`. Das ist ein Mac-Vokabular (`Form Feed`/`Vertical Tab` = Mac-Namen für Bild ab/auf).
- **[BIN]** **Es gibt keinen `Ctrl`-Token im Binary** (2 Treffer, beide unbeteiligt). `Control-` existiert als Modifier-Name, wird aber von **keinem** Werksdefault benutzt. 44 `Command-…`-Kombinationen.
  **[INF, stark gestützt]** `Command` = **Strg** unter Windows. Drei Beine: (a) das Handbuch schreibt `[Ctrl]-C` genau **einmal** auf 280 Seiten (S. 224 bzw. 219, Sound Editor), sonst überall `[Cmd]`/`[Command]`, auch in Windows-Kontexten; (b) SIBIACs Melodyne-4-Mapping („control plus up/down" = grob, „control alt plus up/down" = fein) entspricht exakt `Command-Up` = *Move Up* / `Command-Alt-Up` = *Nudge Up* im Binary; (c) `Command-w/a/z` sind die Windows-Konventionen.
  **Celemony nennt die Zuordnung nirgends.** `[Command]+[Alt]` → `Strg+Alt` **nicht** ungetestet annehmen.

## 3. Werkzeugkasten — Edit-Modus

Vollständiger Baum. Alle F-Tasten sind bei Celemony wörtlich belegt.

| Gruppe | Werkzeug (exakte Schreibweise) | Auswahl | Handbuch |
|---|---|---|---|
| 1 | **Main Tool** | `[F1]`, Toolbox, Kontextmenü | S. 147, `M5tour_ToolMain` |
| 1a | **Scroll Tool** (Hand) | „from beneath the main tool" — **keine F-Taste dokumentiert** | S. 45, `M5tour_PlaybackScrollZoom_studio_2` |
| 1b | **Zoom Tool** (Lupe) | „from beneath the Main Tool" — **keine F-Taste dokumentiert** | ebd. |
| 2 | **Pitch Tool** | `[F2]` | S. 151, `M5tour_ToolPitch_2` |
| 2a | **Pitch Modulation Tool** | `[F2]` ×2 | S. 156, `M5tour_ToolModulationDrift_2` |
| 2b | **Pitch Drift Tool** | `[F2]` ×3 | ebd. |
| 3 | **Formant Tool** | `[F3]` — **einziges Basiswerkzeug ohne Untervariante** | S. 158, `M5tour_ToolFormants` |
| 4 | **Amplitude Tool** | `[F4]` | S. 161, `M5tour_ToolAmplitude` |
| 4a | **Fade Tool** *(neu in 5)* | `[F4]` ×2 | S. 164, `M5tour_ToolAmplitude_Fade_Sibilance` |
| 4b | **Sibilant Balance Tool** *(neu in 5)* | `[F4]` ×3 | S. 166, ebd. |
| 5 | **Time Tool** / Seitentitel **Timing Tool** | `[F5]` | S. 169, `M5tour_ToolTiming_2` |
| 5a | **Time Handle Tool** | `[F5]` ×2 | S. 173, `M5tour_ToolTimeHandlesAttackSpeed_2` |
| 5b | **Attack Speed Tool** | `[F5]` ×3 | S. 174, ebd. |
| 6 | **Note Separation Tool** | `[F6]` | S. 177, `M5tour_ToolSeparations_2` |
| 6a | **Separation Type Tool** | `[F6]` ×2 | S. 178, ebd. |

**[Q]** Die Formant-Nullstelle ist doppelt bestätigt: kein Untertool im Handbuch, und im Binary steht zwischen den Strings `F3` und `F4` nur `handleSelectFormantTool` + `handleSelectToolClassMUFormantTool`. **[FIG]** Der Formant-Button trägt **kein Dropdown-Dreieck** — direkt als Pixelprobe verwertbar.

**Nicht Toolbox-Einträge:** *Formant Transitions Tool* und *Amplitude Transitions Tool* sind das, wozu Formant-/Amplitude-Tool **über dem Notenende** werden ([Q] je 1× im Handbuch).

**[Q]** `[Command]+[Shift]` (Scroll) und `[Command]+[Alt]` (Zoom) sind **temporäre Mausmodi, keine Werkzeugauswahl**: *„Select the Scroll Tool … **or** hold down the [Command] and [Shift] keys …"* — die Toolbox-Auswahl bleibt unverändert. Wichtig für jede Zustandsprobe, die das aktive Werkzeug liest.

**[Q] Versionsdelta 4 → 5:** `Fade Tool` und `Sibilant` haben im Melodyne-4-Handbuch **0 Treffer**. Celemony: *„With Version 5, Melodyne assistant, editor and studio have gained two new tools"* (`M5tour_NewInMelodyne5`). Alles andere existierte in 4 mit gleichen Namen und F-Tasten.

**[Q] 5.3.1:** *„Keyboard shortcuts: The Fade Tool and Sibilant Balance Tool now appear directly beneath the Amplitude Tool, which corresponds to the layout in the toolbox."* → Wenn das Overlay den Shortcut-Baum je nach Position indiziert, muss es ≥ 5.3.1 voraussetzen.

## 4. Werkzeugkasten — Note-Assignment-Modus (anderer Satz!)

**[Q]** `M5tour_NA_Mode_Tools`, PDF S. 78: *„In Note Assignment Mode, the toolbox contains tools with functions other than those used in normal Edit mode… **Which tools are available depends upon the algorithm**."*

Werkzeuge: **Main Tool**, **Activation Tool** (ohne Funktion bei Percussive und Universal), **Note Separation Tool** + **Separation Type Tool** („directly below it in the toolbar") + **Starting Point Tool** („The Starting Point Tool is the **second** sub-tool of the Note Separation Tool"), **Sibilant Range Tool**, **Energy Share Tool**.

**[Q]** Für **keines** dieser Werkzeuge nennt Celemony eine F-Taste — die Seite enthält kein einziges `[F…]`-Token, und „Starting Point Tool"/„Sibilant Range Tool" kommen auf **keiner anderen Handbuchseite** vor.

**[FIG]** Die NA-Toolbar hat **4 Buttons**, nicht 6 (`M5_NA_Mode_Tools_5.1`, 412×136). Ein Flyout mit **3 Einträgen** ist in `M5_NA_Mode_Tools_10.1/12.1/13.1` offen zu sehen. Die Zuordnung Eintrag 1/2/3 → Separation Type / Starting Point / Sibilant Range ist **[INF]** aus der Textreihenfolge, nicht gegen die Bilder belegt.

**[BIN]** `Select Detection AttackAssignment Tool` (vermutlich Starting Point) steht **außerhalb** des `F6`-Blocks, gruppiert mit den anderen Detection-Tools. Der `F6`-Block enthält nur `Time Separation` + `Detection Separation Type`. → **Es gibt keine Binärstütze für `F6`×3 = Starting Point.** [INF] Beste Lesart: Edit-Modus `F6` zykelt **zwei**, NA-Modus hat **drei** in der Gruppe.

**[BIN]** Das Binary listet **acht** Detection-Tools, das Handbuch sieben NA-Tools. `Pitch Relevance` und `Quarter Assignment` haben **0 Handbuchtreffer**. Wenn das Overlay den Shortcut-Baum aufzählt: mit unbenennbaren Zeilen rechnen.

## 5. Wege zur Werkzeugwahl — vier, davon drei ohne Maus

**a) F-Taste, mehrfach [Q]** — „in quick succession". **[LÜCKE]** Das Zeitfenster ist nirgends in Millisekunden beziffert. Achtung: Tastenwiederholung kann still ein Untertool wählen.

**b) Kontextmenü des Note Editors [Q] — auch für Untertools, und ohne Zeitfenster.** Jede Werkzeugseite sagt es; für Untertools explizit: *„Activate the Fade Tool from either the toolbar **or the context menu of the Note Editor** or by pressing the [F4] key … twice"*, ebenso Sibilant Balance, Pitch Modulation, Pitch Drift. **[LÜCKE]** Reihenfolge und Wortlaut der Menüeinträge sind nicht dokumentiert; braucht eine Aufnahme. Achtung §2.3: dieses Menü ist **kein** Win32-Popup.

**c) Eigener Shortcut pro Untertool [Q + BIN] — der robusteste Weg.**
Handbuch: *„From the Preferences dialog, you can also, if you wish, define separate keyboard shortcuts for all three tools."* (Pitch-Seite), analog für die drei Time-Tools, für beide Separation-Tools, für Attack Speed. Preferences-Seite: *„Click on a command (**'Pitch Modulation Tool'** in our example) and then press the key…"*

**[BIN]** Genau **22** Direktauswahl-Befehle existieren:
```
Select Combi Tool                Select Time Tool
Select Scroll Tool               Select Time Handle Tool
Select Zoom Tool                 Select Attack Speed Tool
Select Pitch Tool                Select Time Separation Tool
Select Pitch Modulation Tool     Select Detection Separation Type Tool
Select Pitch Drift Tool          Select Activation Tool
Select Formant Tool              Select Detection Separation Tool
Select Amplitude Tool            Select Detection AttackAssignment Tool
Select Fade Tool                 Select Detection Amplitude Tool
Select Sibilant Balance Tool     Select Detection Quarter Assignment Tool
Select Sibilant Range Tool       Select Pitch Relevance Tool
```
**[INF]** Keiner hat einen Werksdefault (kein `F…`/`Command-…`-String davor, während jeder Gruppen-Zykler einen hat).

**[INF, Empfehlung]** Den Nutzer einmal je Werkzeug eine eindeutige Taste vergeben lassen → jedes Werkzeug per Einzeltastendruck: kein Flyout, keine Koordinaten, kein Doppeldruck-Fenster.

**Gruppen-Zykler im Binary [BIN]**, Stringregion `0x2eb75f8 … 0x2eb7b50`:
```
'F1' → handleSelectCombiTool          'Main Tool'
'F2' → handleSelectPitchTool          'Pitch Tools'
'F3' → handleSelectFormantTool        'Formant Tool'
'F4' → handleSelectAmplitudeTool      'Amplitude Tool'
'F5' → handleSelectTimeTool           'Time Tools'
'F6' → handleSelectTimeSeparationTool 'Separation Tools'
'Command-l' → handleLastTool          'Toggle Last Used Tools'
```

**d) Klick in die Toolbox** — siehe Teil 3, §3.3: die **Geste** ist undokumentiert.

## 6. Anzeigefelder

### 6.1 Modell [FIG — korrigiert die naheliegende Annahme]

**[Q]** `M5tour_NoteInspector`: *„The Note Inspector brings together the inspector fields that are usually displayed near the toolbar when the various tools are in use."* → Der Toolbar-Streifen ist **werkzeugabhängig**.

**[FIG, gemessen in Celemonys 750-px-Toolbar-Crops, alle gleicher Ausschnitt und Maßstab]:**

| Abbildung / Werkzeug | Felder | Innenmaß x |
|---|---|---|
| `M5_ToolPitch_2_3.1` — Pitch Tool | **2** | 286–378 und 394–486 |
| `M5_ToolModulationDrift_2_2.1` — Pitch Modulation | **1** | 286–378 |
| `M5_ToolFormants_2.1` — Formant | **1** | 286–378 |
| `M5_ToolAmplitude_2.1` — Amplitude | **1** | 286–378 |
| `M5_ToolTimeHandlesAttackSpeed_2_3.1` — Attack Speed | **1** | 286–378 |

→ **Ein Slot, der sich nur beim Pitch Tool auf zwei Felder aufspannt.** Alle anderen Werkzeuge: **ein** Feld an derselben linken Position.

**[KAL, 17.08.2026 — korrigiert den Satz darüber]** Die fünf Zeilen der Tabelle sind die fünf
Werkzeug-Abbildungen, die Celemony veröffentlicht. **Das Main Tool ist in keiner davon** — und es
zeigt **zwei** Felder. Gemessen an `modules/overlay-runtime/calibration/Melodyne-clean.png`,
aufgenommen bei aktivem Main Tool (das Pfeil-Icon ist hell hinterlegt): beide Kästchen sind
gezeichnet, in beiden steht ein Strich. Die Kalibrieraufnahme daneben (`Melodyne.png`) setzt je ein
Fadenkreuz mittig in beide Kästchen, also sind es genau diese zwei Regionen.

→ „Nur beim Pitch Tool" beschreibt damit, **welche Abbildungen vorlagen**, nicht welche Werkzeuge
zwei Felder haben. Das Main Tool bearbeitet Tonhöhe, also zeigt es die Tonhöhen-Felder. Das Overlay
hatte das zweite Feld eine Zeit lang auf das Pitch Tool eingeschränkt — die Sperre ist entfernt.

**[LOG, gemessen am 13.08.2026 — die bislang einzige Messung an echtem Melodyne unter Windows]**

Alles oben in diesem Abschnitt stammt aus **macOS-Abbildungen**. Diese Zahlen nicht: sie kommen
aus `target/release/automation-platform.log.1`, aus einer Sitzung, in der jemand Melodyne
tatsächlich benutzt hat. Sie sind **unvoreingenommen**, weil der damalige Modulstand (`03824cf`)
beide Regionen **bedingungslos** las — ohne jede Werkzeugprüfung. Ein leeres rechtes Feld heißt
dort also: auf dem Bildschirm stand nichts.

**178 Messwerte. In 167 davon steht rechts ein Cent-Wert.** Die elf Ausnahmen sind genau die
Ein-Feld-Werkzeuge, erkennbar am linken Feld: `0 ct` (Formant, 4×), `0.00 dB` (Amplitude, 3×),
`100.0 %` (Pitch Modulation, 3×), `0.0 %` (ein Time-Sub-Werkzeug, 1×).

**Das Main Tool zeigt beide Felder — dreimal unabhängig nachgewiesen.** Die Werkzeugleiste
schaltet mit Rechts/Links um eins weiter und läuft um (`switchTab`, Reihenfolge Main, Pitch,
Formant, Amplitude, Time, Note Separation), und die Tastendrücke stehen als
`[keys] dispatch vk 0x27` im selben Log. Von einer Lesung „Note + Cent" führen zwei Rechts auf
Formant (`0 ct`, rechts leer), eines weiter auf Amplitude (`0.00 dB`, rechts leer), drei weitere
laufen über Time und Note Separation zurück auf Main — **und dort steht wieder Note *und* Cent**
(`Db 5` / `-12 ct`). Ein Links vom Pitch-Werkzeug landet ebenso auf Main und liest `B3` / `-1 ct`.

**Die Pitch-Sub-Werkzeuge sind dagegen wirklich einfeldrig**, wie §6.1 sagt: Pitch Modulation
liest `100.0 %` bei leerem rechten Feld, und ein Schritt zurück auf das Basiswerkzeug bringt das
zweite Feld in derselben Sekunde wieder. Die Sperre im Overlay war also zur Hälfte richtig — für
die Sub-Werkzeuge — und für das Main Tool falsch.

**[Q] Celemony bestätigt das Modell**, ohne die Frage zu entscheiden: das Pitch-Kapitel nennt
ausdrücklich zwei Werte — *„die Note und die Cent-Abweichung auch im Inspektor neben dem
Werkzeugkasten"* (DE S. 154; EN S. 152 *„the deviation in cents from equal temperament"*) — und
das Main-Tool-Kapitel erwähnt den Inspector mit keinem Wort. **[≠]** Genau dieses Schweigen ist
aber kein Beleg: Attack Speed und Sibilant Balance haben ebenfalls keinen dokumentierten
Inspector-Abschnitt, obwohl ihre Parameter im Note Inspector aufgezählt sind. Und Celemony sagt
über das Main Tool: *„It has no unique functions but simply offers a different mode of access"*
(EN S. 147), während das Pitch-Kapitel den *pitch center* als *„note parameter that can also be
edited using Melodyne's Main Tool"* beschreibt (EN S. 150) — dasselbe Parameter, dieselben Felder.

**[FORUM] Und die Vorarbeit sagt es auch:** SIBIACs eigenes Modell ist *„Depending from the
tool, zero, one or two parameter controls"* (§13) — pro Werkzeug verschieden, **null** ausdrücklich
eingeschlossen, und ohne Werkzeugnamen. Der AutoHotkey-Entwurf des Nutzers, aus dem die beiden
Regionen stammen, legt beide Anzeigen **bedingungslos** an und fängt ein leeres Feld mit dem
Ersatztext `"No value"` ab — also gerade nicht mit einer Werkzeugbedingung.

→ **[LÜCKE bleibt]** Was **Time** und **Note Separation** in den Kästchen zeigen, ist weiter
unbekannt: bei beiden waren im Log *beide* Felder leer, und das Modul schrieb damals nur eine
Zeile, wenn wenigstens eines etwas enthielt — sie erscheinen also gar nicht. Seit 17.08.2026
schreibt das Overlay bei **jedem Werkzeugwechsel** eine Zeile
`[melodyne] fields under tool=… left=… right=…`, gemessen kurz nach dem Umschalten und
ausdrücklich auch dann, wenn beides leer ist. Die Lücke schließt sich damit beim bloßen Benutzen,
ohne dass jemand auf einen Bildschirm sehen muss.

**Geometrie Pitch-Tool [FIG]:** Feld 1 außen x**284–380** (96 px), Feld 2 außen x**392–488** (96 px) — **gleich breit**, Abstand **12 px**, Höhe außen 24 px (Rahmen y14/y37), innen 21 px. Text horizontal zentriert (Tinte-Mitte 331,0 vs. Feldmitte 332,0 / 441,0 vs. 440,0). Feldinnenfläche Luminanz ≈ 209, Toolbar-Panel 191, Tinte ≤ 130.
**[FIG]** Rechts vom Inspector stehen **weitere Toolbar-Buttons** (~x500–620) — der Inspector ist nicht das rechteste Element.

**[FIG] Note Inspector (Info-Pane), Crop 210×291:** 10 Wertefelder, alle x**104–173** (70 px), ~18 px hoch, Oberkanten y **45, 66, 87 / 116, 137 / 166, 187, 208 / 234, 255**. Werte zentriert (Tinte-Mitte 138–140), Labels rechtsbündig, Doppelpunkt endet x≈93.
**⚠ [FIG] Toolbar-Crop und Note-Inspector-Crop haben unterschiedliche Maßstäbe** (24 px vs. 18 px Feldhöhe, ~11 px vs. ~8 px Versalhöhe). **Zahlen nicht mischen.**

**[Q]** Unter `Sibilant Balance:` folgen Trennlinie und eine `File:`-Zeile: *„Lower down in the inspector, you can see to which audio file the selected note belongs and **which algorithm was used for the detection**."* → **Der Algorithmusname ist aus dem Info-Pane lesbar.**

**[LÜCKE]** Ob **Sibilant Balance** und das **Timing Tool** überhaupt ein Toolbar-Feld haben — weder Prosa noch Abbildung. Für Attack Speed ist es dagegen belegt: `M5_ToolTimeHandlesAttackSpeed_2_3.1` zeigt `0.0 %`.

### 6.2 Was der Note Inspector kann (bester Schreib-/Lesekanal) [Q]

*„It allows you to see all the most important parameters at a glance and even edit them without having to change tools."* Von oben nach unten editierbar:
1. Pitch (Halbtöne / Cent / Hertz) — absolut (`C3`, `D4`) **oder** relativ (`+2`, `-1`)
2. Pitch Modulation (%) 3. Pitch Drift (%) 4. Formant Shift (ct) 5. Amplitude (dB) 6. Mute-Button 7. Attack Speed (%) 8. Sibilant Balance

Je Feld: Ziehen **oder Doppelklick und tippen**. Mehrfachauswahl zeigt `–`; Ziehen am Strich wirkt **relativ**; Tippen von `2` transponiert alle um zwei Halbtöne, `C2` setzt alle absolut.
**Nicht** im Note Inspector: Fades, Zeitposition/-länge, Time Handles, Trennungen.

**[Q]** Im **NA-Modus** sind die drei Pitch-Felder **schreibgeschützt** — dafür gibt es dort zwei Werkzeuge als **Checkboxen**: *„**Hard separation**: The status of this field is determined either by changes made with the Separation Type Tool or by checking/clearing the box…"* und *„**Starting Point**: …"*. Eine Checkbox ist les- **und** setzbar → besseres Ziel als ein Doppelklick auf eine Trennungskoordinate.

## 7. Formate der Anzeigewerte [FIG + Q]

| Feld | Beispielwerte | Regel |
|---|---|---|
| Notenname | `C 3`, `C 4` | **Buchstabe + Leerzeichen + Oktavziffer**, keine Einheit |
| Cent | `+4 ct`, `-3 ct`, `0 ct`, `-122 ct` | **ganzzahlig**; **`+` wird gedruckt** |
| Prozent | `83.0 %`, `-92.0 %`, `12.0 %`, `-26.0 %`, `0.0 %` | 1 Nachkommastelle; **kein `+`** bei positiven Werten |
| dB | `-0.40 dB`, `-4.94 dB` | 2 Nachkommastellen (2 Stichproben) |
| Hz | `261.2 Hz` | 1 Nachkommastelle (**1 Stichprobe**) |
| Sibilant Balance | `-7 %` | ganzzahlig (**1 Stichprobe**, schwach belegt) |

- **Wert und Einheit immer durch Leerzeichen getrennt.**
- **Das Leerzeichen in `C 3` ist ein echtes Zeichen** [FIG]: Spalte `C` = x321–329, Lücke x330–334 (**5 px**), `3` = x335–341. Zum Vergleich: Wortabstand in `+4 ct` ebenfalls 5 px, Kerning innerhalb eines Tokens 1–2 px. Für den Parser relevant.
- **Mehrfachauswahl = ein zentrierter Strich** [FIG], gemessen **5×2 px** (Toolbar) — zwei Abbildungen, vier englische Seiten und die deutsche Seite sagen „ein Strich"; **[≠]** genau eine englische Seite (Pitch Tool) sagt *„three hyphens"*. → Als **einen** Strich behandeln, im Parser aber `-`, `–`, `---` tolerieren.
- **Hz ist keine feste Funktion aus Note+Cent** [Q]: die Referenztonhöhe ist pro Dokument einstellbar (Concert A4=440 / Default aus Preferences / Detected), Stimmgabel-Icon oben am Reference Pitch Ruler (S. 117).
- **Notennamen-System ist umschaltbar** [Q, Preferences]: *„Pitch labels: … English (C, B, Bb etc.), German (C, H, B etc.) or Latin (Do, Si, Sib etc.)"*. → **Entweder die Einstellung festnageln oder alle drei Wörterbücher mitführen.**
- **[FIG]** Der Pitch Ruler schreibt Vorzeichen als ASCII `b` (`Db`), kein `♭`-Glyph. **[INF]** Dass das **Inspectorfeld** genauso schreibt — ungeprüft.
- **Mittleres C = C4** [Q, `M5tour_Multitrack_2`: *„all instances of Middle C (C4)"*].
- **[Q/INF]** Bei den Algorithmen **Percussive** und **Universal** zeigt der *Pitch Ruler* nur relative Halbtonwerte statt Notennamen. **Percussive Pitched ist ausdrücklich anders** („detected sounds are separated and assigned to individual pitches"). Dass das **Inspectorfeld** dem folgt, ist **[INF]**.
- **[Q]** Ausgegraut: Attack Speed bei Universal; Sibilant Balance nur bei Melodic/Percussive Pitched; Algorithm-Menü bei leerer Auswahl oder Auswahl aus zwei Quellen.

## 8. Werks-Tastaturbelegung 5.4.1 [BIN, `MDActionPool.gnui`]

`Command-` = Strg lesen (§2.4). **[Q]** markiert = zusätzlich im Handbuch belegt.
**Wichtige Einschränkung der Methode:** Der Stringpool ist **dedupliziert** — Titel, die anderswo im Binary stehen (Undo, Cut, Copy, Paste …), fehlen in der Sequenz und wurden aus Handlernamen rekonstruiert.

**Transport** [BIN + Q]: `Space` Play/Pause · `Return (Num)` Start · `0 (Num)` Stop (2× → Projektanfang) · `Alt+Space` Playback Selection. **Ohne Default:** Record, Toggle Cycle, Toggle Click, Set Cycle To Selection.
**[Q]** Das Handbuch rahmt die Ziffernblock-Liste als **Stand-alone**; das ARA-PDF hat **gar keine** Tastatur-Transportliste (dort: Doppelklick in den Note-Editor-Hintergrund).

**Navigation/Auswahl** [BIN]: `Left/Right`, `Up/Down` = Select · `Shift+…` = Auswahl erweitern · `Strg+A` Select All. **Ohne Default:** Invert Selection, Select Successive Elements, Same Pitch Index/Class/Fifths/Instruments/Beat, Between Locators, Reveal Covered Elements.

**Werte ändern** [BIN]: `Strg+Pfeile` = Move (grob) · `Strg+Alt+Pfeile` = Nudge (fein). **[Q]** *„As is the case in the Note Editor, you can access most of the functions of the current tool using the [Cmd] and arrow keys… holding down the [Alt] key permits finer adjustment"* (S. 234). **[INF]** Was die Pfeile tun, hängt vom aktiven Werkzeug ab.
**[FORUM, konkrete Schrittweiten, Melodyne 4]** grob: 1 Ganzton / 1 dB / 50 % / 100 ct — fein: 1 ct / 1 % / 0,1 dB.

**Clipboard/Edit** [BIN]: `Strg+Z` Undo · `Strg+Shift+Z` Redo · `Strg+Y` Redo (alt.) · `Strg+X/C/V` · `Strg+D` Duplicate · `Delete` / `Backspace` · **`Return` (Haupttastatur) = „Start Editing"** (numerisches `Return` = Transport Start — **zwei verschiedene Einträge**). **Ohne Default:** Quantize Pitch, Quantize Time, Note Leveling, Separate/Merge Note, Trills.

**Zoom/Ansicht** [BIN]: `Strg + Num +` Zoom In · `Strg + Num -` Zoom Out. **[BIN, wichtig]** **Es gibt im Action Pool überhaupt keine Scroll-Aktionen** — nicht „keine Defaults", sondern keine Befehle. Scrollen geht nur über das Scroll-Tool bzw. Maus. → **Ein reiner Tastaturnutzer kann nicht scrollen; die vier gerichteten Zoom-Befehle müssen von Hand belegt werden.**

**Datei** [BIN]: `Strg+N/O/W/S`, `Strg+Shift+S`, `Strg+Q`.
**Tracks** [BIN]: `Strg+Alt+N/M/S/R/G/E`.
**NA-Modus** [BIN]: `Strg+Shift+N` = `handleToggleDetectionEditor` — **im Handbuch nicht erwähnt**; `Strg+Alt+A` = Insert Attack at Cursor and Split Note.
**Framework (nicht MDActionPool)** [BIN]: `Command-,` Preferences · `Command-m` Miniaturize · `Command-Alt-Shift-b/s/d/e/f/p` Debug. **[LÜCKE]** Ob diese unter Windows überhaupt aktiv sind und ob sie in der Shortcut-Liste auftauchen — ungeprüft.
**Mehrdeutig** [BIN]: `Command-Shift-t` / `Command-Shift-c` liegen neben `Arm Transfer` und `Clear Track-Transfer Buttons` — **welche Taste zu welcher Aktion gehört, ist aus dem Dump nicht bestimmbar.**

### 8.1 Zwei Fallen

**⚠ Plug-in lädt einen DAW-spezifischen Satz** [Q, „New in Version 5.3"]: *„When Melodyne is employed for the first time as a plug-in, it loads the set of keyboard shortcuts corresponding to the DAW you are using."* → **F1–F6 sind im Plug-in nicht garantiert.** Mitgelieferte Sätze [BIN, UTF-16 @ `0x37b49f5`–`0x37b54d1`]: `01_Melodyne 4`, `02_Melodyne 5`, `03_Pro Tools`, `04_Logic Pro`, `05_Studio One`, `06_Cubase` (Inhalte komprimiert, **nicht gelesen**). Host-Erkennungstabelle [BIN]: `tools→Pro Tools`, `logic→Logic Pro`, `cubase`, `studio one`, `cakewalk→Sonar`, `live`, `cockos→Reaper`, `mixcraft`/`acoustica`, `Nuendo`, `garageband`, `samplitude`, `bitwig`, `performer→Digital Performer`, `tracktion`, `FL Studio`. **Das Overlay muss die Belegung lesen, nicht annehmen** — und es kann sie lesen: die Shortcut-Seite nennt den geladenen Satz („Cubase", „Cubase (edited)", „Melodyne 5", S. 63).

**⚠⚠ Die Shortcut-Seite ist ein permanenter Lernmodus** [FORUM, beide Quellen]: *„In case some action is currently in learning state, **any key or combination, including Escape and Alt+F4 will be assigned to this action!** There is no way to tell Melodyne to stop learning, except by changing the page."* Der Zustand überlebt das Schließen des Dialogs. → **Jede synthetische Eingabe hart daran koppeln, dass diese Seite *nicht* im Fokus ist.**

**[BIN, nicht dokumentiert]** Die Shortcut-Seite hat einen **Text-Export**: Lokalisierungsschlüssel `GNShortCutOutlineController.Window.PullDownButton.handleImport` / `.handleExport` / `.handleExportText`, Aktion `exportShortcutsAsText`. Sichtbare Item-Labels im Stringpool: `Import ...`, `Export ...`, `Clear Selected Short Cut`, `Reset to Factory Defaults`, `Options`, `edited...`. **[LÜCKE]** Das Label des dritten Eintrags ist unbekannt — „Export as Text" wäre geraten. **Wenn er funktioniert, ist das die vollständige Live-Tabelle als Textdatei.**

## 9. Layout-Gefahren — alles, was Koordinaten verschiebt

Alle **[Q]**, sofern nicht anders markiert:

1. **Info-Pane ist verschiebbar** — Stand-alone: *„left and/or right (full height / top half only / bottom half only)"* (S. 51) ≈ 7 Zustände, verschiebt **linke und rechte** Kante des Note Editors. **In ARA nur ein Ein/Aus-Schalter** (ARA S. 49).
2. **Projekt-Tabs erscheinen nur bei mehr als einem offenen Projekt** (S. 41) → Vertikalversatz von allem darunter.
3. **Show Tracks / Show Note Editor / Show Sound Editor / Show Tempo Editor** verteilen die Höhe neu; die Trennlinie Tempo/Note ist ziehbar (S. 233).
4. **Scale Editor: „successively one, two or all three panes"** (S. 51) — dreistufig im Stand-alone, in ARA ein Schalter für alle drei Spalten. Verschiebt die linke Note-Editor-Kante.
5. **Key- und Chord-Track schalten unabhängig** (S. 51).
6. **Toolbox-Inhalt wechselt zwischen Edit und NA-Modus** (S. 78) **und innerhalb NA je Algorithmus** (S. 78, 86, 90).
7. **Zusätzlicher Slider neben der Toolbox** — bei polyphonem Material mit Main/Activation (S. 81), mit zwei Indikatoren bei Separation-Tools (S. 84).
8. **Note-Editor-Optionen werden pro Modus getrennt gesetzt** (S. 51) → dieselbe Probe kann in Edit und NA unterschiedlich lesen.
9. **Frei resizebar** an der rechten unteren Ecke, Stand-alone wie Plug-in (S. 45, 47). **[LÜCKE]** Keine dokumentierte Mindestgröße. Die Top-Bar besteht aus **19** benannten Zellen [BIN: `MDToolbarTransportCtrl`, `…TempoCtrl`, `…ToolCtrl`, `…GridCtrl`, `…PositionCtrl`, `…EditModeCtrl`, `…EditMixCtrl`, `…VolumeCtrl`, `…UndoCtrl`, `…QuantizeMacrosCtrl`, `…ScaleModeCtrl`, `…PreListeningCtrl`, `…ActivityCtrl`, `…EdtionCtrl` [sic], `…EditorViewCtrl`, `…TrackMemoryUsageCtrl`, `…LeftConfigCtrl`, `…RightConfigCtrl` + 6 ARA-Varianten] → **mit Umbruch rechnen.**
10. **Appearance (Kontrast) und Language gelten für Stand-alone und Plug-in gemeinsam** (S. 57) — eine Änderung verschiebt Referenzfarben bzw. OCR-Strings auf beiden Seiten.
11. **Startpanel beim Programmstart** [BIN `MDWelcomePanelController.gnui` ×2 Größen; FORUM: dauerhaft abschaltbar per Checkbox; plist-Schlüssel `disableWelcomePanelOnStartup`].
12. **Sound-Editor-Arbeitsbereiche sind Tabs, mehrere gleichzeitig nebeneinander möglich** (`[Command]`-Klick, S. 211).
13. **Der Time-Grid-Regler existiert zweimal** — einmal im Track-Pane, einmal im Note Editor (S. 101): *„The sole reason for the grid appearing in both panes is to ensure it remains accessible when either pane is hidden."*
14. **Die rechte untere Note-Editor-Ecke trägt zwei Steuerelemente** — Auto-Scroll-Zustandsicon (S. 47/51) **und** Blob-Höhen-Slider (S. 46).
15. **[FIG, quellenübergreifend beobachtet, nicht dokumentiert]** Die Buttongruppe **links** der Werkzeuge hat im **Stand-alone 2 Buttons** ([Zwei-Blob][Schraubenschlüssel]), im **ARA-Plug-in 3** ([Zwei-Blob][Ein-Blob][Schraubenschlüssel]) — beobachtet in `M5-Legendenbild` + `M5_ViewOptions-stand-alone_1.1_studio` gegen den MusicTech-2560er-Screenshot. **Wenn das allgemein gilt, verschiebt sich jeder absolute x-Offset der Werkzeuggruppe um eine Buttonbreite.** → **Immer am linken Rand der Werkzeuggruppe verankern, nie an der Fensterkante.**
16. **Windows-Anzeigeskalierung** (§2.4).
17. **[Q]** „Show Replace Ranges" (Options > Note Editor, nur Transfer-Plug-in) fügt Markierungen im Note Editor hinzu.
18. **[Q]** „Show Blob Info" blendet einen lokalen Pitch Ruler vor dem Blob plus Ziehzonen-Linien ein (S. 54) — stört Pixelproben nahe der Blobs.

**[LÜCKE, höchster Wert für ein Pixel-Design]** Was die **Appearance-Kontrasteinstellungen** genau sind — Anzahl, Namen, Wirkung. Das Handbuch gibt einen Satz (S. 57). Das Binary trägt zusätzlich undokumentierte Farbkorrektur-Controller (`MDColorCorrectionPrefCtrl`, `MUColorCorrectionPrefCtrl`, `MUColorCorrectionCtrl`).

## 10. Zustandsproben ohne OCR [Q]

- **Note-Editor-Hintergrund wechselt die Farbe** zwischen Edit- und NA-Modus (S. 74) — die primäre Modusprobe.
- **Auto-Scroll-Icon rechts unten wechselt die Form**, wenn Auto-Scroll ausgesetzt ist (S. 47, 51).
- **Tempo-Feld-Präfix**: `=` konstant, `~` variabel (S. 35, 238).
- **Algorithm-Menü ausgegraut** bei keiner Auswahl oder Auswahl aus zwei Quellen (S. 70) → Auswahlzustandsprobe.
- **Orangefarbener Fokusrahmen** [Q, S. 43]: *„The pane with focus at any given moment is the one enclosed in a thin orange frame."* — dokumentierte, pixelprüfbare Fokusanzeige.
- **[FIG]** Der Elternbutton in der Toolbox **rendert das Icon des aktiven Untertools** (vgl. `M5_ToolPitch_2_1.1` vs. `M5_ToolModulationDrift_2_2.1`) → verrät, welches Untertool aktiv ist, macht aber Template-Matching auf „den Pitch-Button" unmöglich.
- **[FIG]** Formant- und Edit-Mode-Button haben **kein Dropdown-Dreieck**, die anderen schon (unabhängig bestätigt durch die protoolstraining-Crops).

## 11. Analyse-/Detection-Phase

**[Q]** *„Melodyne analyzes the audio material to find the notes it contains… We call this process 'detection'."* (`M5tour_AudioAlgorithms`)
**[Q]** *„Since for this analysis the audio file has to be examined as a whole, it cannot be conducted in real time; it is performed once only, at the start, before the first blobs appear in the Note Editor… In the stand-alone implementation of Melodyne, this is when the audio file is first opened."* (`M5tour_TransferARA` — **nur im Help Center, in keinem PDF**)

**Wann sie läuft:** Laden/Transfer; **jeder Algorithmuswechsel** (*„Warning: Any time you switch algorithms, all editing previously performed … is lost!"*); „Analyze Chords"/„Analyze Key"; im Plug-in verschiebbar per Preference „Detect audio after transfer".
**Zweiphasig** [Q]: Bei „Automatic" prüft Melodyne zuerst auf Polyphonie („only after a great deal of processing"), dann kann *„the polyphonic detection process will be interrupted and a fresh detection … using the Percussive Algorithm … will commence"*.
**Kostenmodell** [Q]: *„the more notes a file contains, the longer the detection process takes… with files longer than an hour, the detection process is generally slow; files longer than two hours … may be impossible to load or transfer at all"* → brauchbar für Timeouts.

**[LÜCKE — vollständig]** Der Fortschrittsindikator. Gründliche Suche über 280 Seiten: `spinner` 0, `hourglass` 0, `busy` 0, `Analyzing` 0, `Detecting` 0, `Please wait` 0, `percent complete` 0. Der **einzige** Handbuchtreffer für „progress indicator" betrifft eine **Rechenpause innerhalb des NA-Modus** (`M5tour_NA_Mode_2`), nicht die Lade-Detection. *„Polyphonic Detection"* existiert **nicht** als großgeschriebenes UI-Label; kleingeschrieben kommt „polyphonic detection" 3× in der Prosa vor. Die einzigen großgeschriebenen `… Detection`-Labels sind **„Sibilant Detection"** (3×) und **„Tempo Detection"** (2×).
**[FORUM]** SIBIAC hat aufgegeben: *„Sibiac does not support the description or reporting of the analysis progress."* / *„During that time you can not edit anything."* → **Keine Vorarbeit zum Abschauen.**

**[INF, aber besser als Pixel]** Detection schreibt in den Audio-Cache → **Dateisystem-Watch statt Bildschirm.** Siehe §12 und den Pfadwiderspruch in §3.7.

## 12. Dateien statt Bildschirm

**[BIN/Datei, geprüft]** `C:\Users\<user>\AppData\Roaming\Celemony Software GmbH\com.celemony.melodyne.plist` — **Klartext-XML-Plist**, 7 KB, 113 Schlüssel. Direkt nutzbar:
`AppPositionLeft`, `AppPositionTop`, `AppPositionWidth`, `AppPositionHeight`, `AppShowState` → **die Fenstergeometrie lässt sich lesen statt messen**; `MDPrefsMainResponderViewIdKey` (persistierter First Responder); `MDPrefsEditorVisibleKey`, `MDPrefsHeaderVisibleKey`, `MDPrefsSpectrumShaperVisibleKey`, `MDPrefsArrangerVisibleKey`, `MDPrefsLeftInspectorPosKey` / `RightInspectorPosKey` → **Pane-Zustand ohne Bildschirm**; `MDLastModeIsTrackMode`; `disableWelcomePanelOnStartup`.

**[INF]** Angepasste Shortcuts landen vermutlich in derselben Datei — das Binary trägt die Preference-Keys `GNShortCuts`, `GNShortCuts5`, `DefaultGNShortCuts5`, `GNShortCutsTitle`, `GNNotEditedKeySequences` und die Regex `[0-9]+_`. **Auf dieser Maschine ist keiner davon in der Plist** (nie ein Satz angepasst) → **Format unbekannt.** Sobald der Nutzer einmal speichert: Plist diffen.

**[Q]** Gespeicherte Shortcut-Sätze sind ein Dokumenttyp: `Melodyne Shortcuts File Format`, UTI `com.celemony.melodyne.shortcut`, Endung `.shortcuts`. Der im Handbuch (S. 63) genannte Windows-Ordner `C:\ProgrammData\CelemonySoftwareGmbH\Shortcuts\Melodyne5` ist **doppelt falsch** — Tippfehler „ProgrammData", und der reale Ordner ist `C:\ProgramData\Celemony Software GmbH\` (mit Leerzeichen), der hier nur `Licenser` und `Productinfo` enthält.

**[Q, aber §3.7 beachten]** Audio-Cache, laut PDF S. 58 **fest und nicht änderbar**:
`C:\Users\USERNAME\AppData\Local\com.celemony.melodyne\Separations`

## 13. Vorarbeiten Dritter — Stand und Verwertbarkeit

**SIBIAC (Alexey Zhelezov, azslow3)** — NVDA-Add-on, **Windows only**, OCR (Tesseract) + NVDA-Textbereiche.
- **Modell, jetzt dokumentiert statt vermutet:** Add-on-Verzeichnis: *„Allows working with **pre-defined graphic and mouse only fixed dialogs**"* (nvda-addons.org id=215). Autor: *„the interface is mostly fixed dialogs. So looking at the interface **I extract that information manually and write it into particular layer definition**."*
- **Melodyne-Ziel: Version 4 studio.** Pseudo-Controls: „Top menu", „Edit Tools"-Switch (Main, Pitch, Pitch modulation, Pitch drift, Formant, Amplitude, Time, Attack speed), „Edit", *„Depending from the tool, zero, one or two parameter controls"*. Tab/Shift+Tab zykeln, Up/Down ändern, Enter öffnet ein **SIBIAC-eigenes** Eingabefeld („This is sibiac specific functionality" — **nicht** Melodynes „Start Editing").
- **Ausdrücklich nicht abgedeckt:** Mehrspur (*„Even in Studio, I do not support multiple tracks"*), *„most features of Studio"*, Analysefortschritt, Notentrennung (Wunsch von Felipe Zanabria 03.07.2022, nie umgesetzt), Tempo-Assign-Bildschirm. Sprache nur Englisch/Spanisch. Fenster darf nicht verdeckt sein; Größenänderung nach dem Start macht SIBIAC blind.
- **Layout-Rezept (übernehmenswert):** „Show tracks" **aus**, „Show Note Editor" **an**, unter „Show info panel" **nichts**, unter „Show tempo editor" **nichts**; Windows-Skalierung 100 %; Monitor ≥ 1920×1080; Fenster maximiert und danach nicht mehr ändern.
- **Stand:** aktuell ist **`sibiac_x64.0.23p2b18.nvda-addon`, 168 kB, 14.04.2026** (plus `sibiac_ocr_x64.0.1p6`, 18 MB). Das NVDA-Verzeichnis zeigt veraltet 0.23p2b8. Melodyne-spezifisches steckt komplett in den 168 kB, **GPLv2**, in `AppModules/sibiac/__init__.py` — **[LÜCKE]** ungelesen, Download bräuchte Freigabe. Das Autorenwerkzeug (`sibiac.exe`/`sibiact.exe`) wurde zurückgezogen; **es gibt keine veröffentlichte Koordinatentabelle.**
- Autor selbst, 23.05.2022: *„From the beginning on, I knew that SIBIAC approach in its current form is not really the way to go."* und 26.08.2025: *„I am not using this software for myself."*

**LBL (reaperaccessible.fr)** — hat SIBIACs Code integriert („LBL supports the same plug-ins as Sibiac as well as Kontakt full versions 6 and 7"), nicht gleichzeitig mit SIBIAC aktivierbar, freie Software, aber **nicht** im NVDA-Review-Prozess. Melodyne wird auf der Produktseite **nicht** genannt; der einzige Beleg ist die Changelog-Zeile *„2023-03-05 — LBL update version 1.19, Sibiac update for Melodyne 5"*, die inzwischen **aus dem Live-Changelog gelöscht** wurde (63 → 46 Einträge; Nachbareinträge überlebten). Nur noch im Archiv: `web.archive.org/web/20250210000540/…id=442…`

**JAWS-Skripte (Steve Spamer, samplitudeaccess.org.uk)** — „Melodyne V5 Jaws Script Solution", empfohlen 20.11.2023 im rwp-Forum. **Domain ist heute eine Casino-Seite; 542 archivierte URLs, kein einziger Treffer auf „melodyne" → die Dokumentation ist verloren.** Erhalten sind **zwölf** YouTube-Titel (Kanal „Samplitude Access", alle 19.09.2022) und ihr identischer Beschreibungsblock: *„The Jaws scripts support all versions of Melodyne, (Essential, Assistant, Editor and Studio), along with all 3 useage possibilities, (stand alone, ARA integration and the VST plugin)."* — **Herstellerselbstaussage, keine unabhängige Bestätigung.** Dagegen Scott Chesworth, 20.11.2023: *„these JAWS scripts for Melodyne, while being very fully featured, **aren't compatible with Melodyne running in REAPER yet**."* → Die Bruchlinie ist **hostspezifisch**, nicht „ARA ja/nein".

**ReaHotkey** (für den Port relevant): Melodyne steht dort **nur unter „Roadmap"**, ausdrücklich ohne Zusage. Seine vier Control-Familien (Basic/Custom, **Graphical** = Bildsuche in Region, **Hotspot** = feste Koordinate, **OCR** = Region ansagen) plus **Farbprobe** (`Colors when the control is on/off`, `checked/unchecked`) sind die Bausteine, die hier gebraucht werden.

**HotSpotClicker (JAWS-Engine)** — das Positionierungsmodell ist das übernehmenswerte Stück: *absolute Bildschirmkoordinate* / *Application mode* (relativ zur App-Kante) / *topLevel* (relativ zum Dialog) / *Current Window* (kleinstes Fenster unter dem Cursor) — plus *„can be caused to adjust the location by searching near that initial location for **text strings, graphic names, or pixel colors**"*, plus benannte Hotspot-**Sets**, die bei Fokuswechsel umschalten. **Genau das Gegenteil von SIBIACs starrem Modell — und SIBIACs Autor sagt selbst, dass sein Modell an dynamischen UIs scheitert.**

## 14. Bildmaterial — was existiert und was drauf ist

**CDN-Muster** [Q]: `https://assets.celemony.com/code/<ASSET_ID>?language=<en|de|fr|es|ja>&dppx=<1x|2x>`
- `dppx=2x` liefert exakt doppelte Maße; `3x`/`4x` fallen still auf 1× zurück.
- **`language` ist dekorativ** — `en`/`de`/`fr`/`es`/`ja` liefern **bytegleiche Dateien**.
- **`env=` ändert Prosa, nicht Abbildungen** — identische Asset-Listen unter `standAlone`, `dawsWithAra`, `reaper`, `logic`, `proTools`.
- `melodyneEssential5` gibt für `M5tour_ToolMain` **404** (andere Doc-Keys oder anderer Slug).

**Offene Flyouts — es gibt sie, entgegen der naheliegenden Annahme** [FIG, angesehen]:

| Asset | 2×-Maße | Inhalt |
|---|---|---|
| `M5_ToolModulationDrift_2_1.1` | 1500×252 | Pitch-Flyout **offen**, 2 Einträge |
| `M5_ToolTimeHandlesAttackSpeed_2_2.1` | 1500×246 | Time-Flyout offen, Eintrag 1 markiert |
| `M5_ToolTimeHandlesAttackSpeed_2_3.1` | 1500×282 | Time-Flyout offen, Eintrag 2 markiert; Feld `0.0 %` |
| `M5_ToolAmplitude_Fade_Sibilance_1.1` | 1500×224 | Amplitude-Flyout offen, **Mauszeiger sichtbar** |
| `M5_ToolAmplitude_Fade_Sibilance_5.1` | 1500×306 | dito, Eintrag 2; Feld `0 %` |
| `M5_ToolSeparations_2_4.1` | 1500×190 | Separation-Flyout offen — **ein** Eintrag |

**Flyout-Geometrie** [FIG, `M5_ToolModulationDrift_2_1.1` @2×]: Elternbutton y≈21–83, Eintrag 1 y≈85–147, Eintrag 2 y≈149–211 → je ~62 px @2× (~31 px @1×), **gleiche Breite wie der Elternbutton, linke/rechte Kante bündig, senkrecht nach unten**. Buttonraster x ≈ 176, 238, 300, 362, 424, 487, 549 @2× → ~62 px @2×, **~31 px @1×**.

**`CMS-Toolkit_M5`** (938×697 @2×, auf `celemony.com/en/melodyne/what-is-melodyne`) — **die wertvollste Einzelabbildung**: ein Explosionsdiagramm der **gesamten Toolbox mit allen Flyouts gleichzeitig aufgeklappt**. Zeile 1 = sechs Elternbuttons, Zeile 2 = jeweils erstes Untertool, Zeile 3 = jeweils zweites, mit Lücken. Bestätigt den kompletten Baum optisch. **Aber: komponierte Marketinggrafik, kein Screenshot eines echten Flyouts.**

**Weitere brauchbare Vollbilder:** `M5_ViewOptions-stand-alone_1.1_studio` (1500×948 @2×) = **beschriftete Gesamtübersicht mit Callouts A–M**, allerdings **deutschsprachiges UI** trotz `language=en`; `M5-Legendenbild` (1346×1650 @2×) = unbeschrifteter Stand-alone-Screenshot; MusicTech-Hero 2560×1707 (`musictech.com/reviews/plug-ins/the-big-review-celemony-melodyne-5/`, ARA auf macOS, Toolbar voll lesbar); Sound-on-Sound-Maximum **1000×605** über `styles/news_large/`.
**Wertlos:** `Tour-Intro-Bild_studio_quickstart2` — unscharfes, perspektivisch verzerrtes Marketingbanner.

**Isolierte Icons** [Q, DOM-`alt`-Attribute]: `protoolstraining.com/images/2022/Melodyne_3_Article/` — `04_met.png`=Main, `05`=Scroll, `06`=Zoom, `07`=Pitch, `10`=Pitch Modulation, `13`=Pitch Drift, `16`=„Format" [sic] Tool, `18`=Amplitude, `21`=Fade, `30`=Sibilant Balance, `34`=Time, `39`=Time Handle (**52×52**, alle anderen 26×26), `46`=Attack Speed, `50`=Note Separation, `54`=Separation Type, `03`=Edit Mode. **Kein Hotlink-Schutz** (curl ohne Referer → 200). Die Crops zeigen **ganze Buttons inkl. Rahmen und Dropdown-Dreieck**, nicht nackte Glyphen.

**Videos** [Q]: `https://assets.celemony.com/code/<FILM_ID>?language=en` leitet auf reine MP4s um — `M5film-TuningTools` → `.../TheTuningTools.mp4` (**758 MB**), ebenso `TheTimingTools`, `TheBasicWorkflow`, `TheLevelingTools`, `TheCreativeUseOfNoteSeparations`, `AQuantumLeapInVocalEditing`. **Nicht heruntergeladen.**
**Freie Nutzung** [Q]: `celemony.com/en/service1/about-celemony/resources` — *„These may be used free of charge in all kind of media"*, enthält „The logo and screenshot", ZIP ≈ 8 MB: `Celemony_Melodyne_5_Product_Info_and_Pictures.zip`. **Inhalt ungeprüft.**

---

# TEIL 2 — WAS OFFEN IST: Fragenkatalog für eine Sitzung an der Maschine

Nach Nutzen sortiert. Jede Frage ist so gestellt, dass sie in einem Durchgang beantwortbar ist. **Vorher das Layout festnageln** (SIBIAC-Rezept, §13) und **die DPI mitschreiben**.

### A. Fenster und Win32 (kein Bildschirmzugriff nötig, kein Sehen nötig)
1. **Welche Klasse trägt welches HWND?** Alle Top-Level- und Kindfenster von `Melodyne.exe` aufzählen: welches ist `GNWindowDoc`, welches `GNWindow`, welches `GNWindowMenu`? Gibt es ein `GNEmbedded…`-Kind? — *Danach im DAW wiederholen: welche Klasse hat der Plug-in-Editor?*
2. **Wie lautet der Fenstertitel** (Stand-alone leer/„Untitled"/Dateiname)? Nur zur Kenntnis — nicht zum Matchen.
3. **Liefert `GetMenu(hwnd)` ein gültiges Menü?** Aufzählen mit `GetSubMenu`/`GetMenuItemInfoW`: **wie heißen die Top-Level-Einträge tatsächlich** (File, Edit, Options, Track, … gibt es ein „Algorithm"-Menü? gibt es „Help"?), und lassen sich Einträge per `WM_COMMAND` auslösen?
4. **Erzeugt ein Rechtsklick im Note Editor ein neues Win32-Fenster?** (Erwartung: nein.) Falls doch — welche Klasse?
5. **Öffnet `F10` oder `WM_SYSCOMMAND/SC_KEYMENU` die Menüleiste?** (Alt tut es nachweislich nicht.)
6. **Löst `PostMessage(WM_KEYDOWN)` Shortcuts aus, `SendMessage` nicht?** Ein Test mit `Strg+A` genügt.
7. **Ist die erste Taste nach Fensteraktivierung verloren?** (Behauptung ohne jede Quelle.) Zehnmal aktivieren + sofort `F2` → wie oft greift es?

### B. Tastatur (der wichtigste Block — er entscheidet, ob Koordinaten überhaupt nötig sind)
8. **Ist `[Command]` unter Windows `Strg`?** Prüfen an: `Cmd+Shift`+Ziehen (Scroll), `Cmd+Alt`+Ziehen (Zoom), `Cmd+W`, `Cmd+A`, `Cmd`+Klick auf einen Edit-Button.
9. **Welche der beiden Zoom-auf-Auswahl-Kombinationen stimmt** — `Cmd+Shift`+Doppelklick (S. 45) oder `Cmd+Alt`+Doppelklick (S. 47)?
10. **Funktioniert `F1` ×2/×3 als Scroll/Zoom?** (Doppelt gestützte Inferenz: defkey-Label „Main / Scroll / Zoom tool" für Melodyne 4 **und** die Binärlage der Strings — aber nirgends dokumentiert.)
11. **Wie viele Untertools zykelt `F6` im Edit-Modus — zwei oder drei?** Kommt `F6`×3 beim Starting Point Tool an? (Binärlage spricht dagegen.)
12. **Wählt die bloße Taste `5` das Time Tool** (wie die Timing-Seite durch einen CMS-Fehler behauptet), oder nur `F5`?
13. **Wie groß ist das Doppeldruck-Zeitfenster** („in quick succession") in Millisekunden? Grob eingrenzen: 150 / 250 / 400 / 600 ms.
14. **Existiert der Text-Export der Shortcut-Liste, und wie heißt der Menüpunkt?** Preferences → Shortcuts → Zahnrad/Pulldown, dritter Eintrag. **Wenn ja: Datei exportieren — sie ersetzt Kapitel 8 dieses Dokuments durch Grundwahrheit.** *(Vorsicht: Lernmodus, §8.1 — auf dieser Seite keine synthetischen Tasten senden.)*
15. **Zeigt die Shortcut-Seite unter Windows „Command-z" oder „Ctrl+Z"?**
16. **Vergibt Melodyne pro Untertool wirklich einen eigenen Shortcut?** Einmal `Pitch Modulation Tool` eine Taste zuweisen, speichern, Plist auf `GNShortCuts5`/`GNShortCutsTitle` diffen → **Speicherformat der Zuweisungen** (bisher völlig unbekannt).
17. **Sind `Command-,` / `Command-m` / `Command-q` unter Windows aktiv, und tauchen sie in der Shortcut-Liste auf?**
18. **`Strg+Shift+T` vs. `Strg+Shift+C`** — welche ist „Arm Transfer", welche „Clear Track-Transfer Buttons"?

### C. Toolbox und Flyout
19. **Welche Mausgeste öffnet das Flyout?** Nacheinander: kurzer Klick / **Klicken und halten** / Rechtsklick / Klick aufs Dreieck. **Und: bleibt es nach dem Loslassen offen (Klick-Klick), oder braucht es Ziehen?** — *Das ist die Kernfrage zu Annahme 3 in Teil 3.*
20. **Was steht im Kontextmenü des Note Editors, in welcher Reihenfolge, mit welchem Wortlaut?** (Untertools sind dokumentiert enthalten.) Screenshot + OCR reicht.
21. **Native Pixelmaße:** Buttonbreite/-höhe/-raster der Toolbox, Flyout-Eintragshöhe, Position des ersten Buttons relativ zur linken Kante der Werkzeuggruppe — **auf Windows, bei bekannter DPI**. Drei Fremdquellen sagen 22/26/31 px; keine gilt.
22. **Hat die Gruppe links der Werkzeuge im Stand-alone 2 und im Plug-in 3 Buttons?** (Nur aus Bildern beobachtet.)
23. **NA-Modus: wie viele Buttons hat die Toolbox tatsächlich, und was steht im 3-Einträge-Flyout in welcher Reihenfolge?**

### D. Anzeigefelder
24. **Was zeigen die Felder, wenn nichts ausgewählt ist?** Leer, letzter Wert, Strich? — *Erschöpfend im Handbuch gesucht, nicht vorhanden.*
    → **[KAL, 17.08.2026] Beantwortet: ein Strich, in BEIDEN Feldern.**
    `modules/overlay-runtime/calibration/Melodyne-clean.png` zeigt bei leerer Auswahl zwei
    gezeichnete Kästchen mit je einem Strich — nicht leer und nicht der letzte Wert.
25. **Haben Sibilant Balance und das Timing Tool überhaupt ein Toolbar-Feld?**
26. **Schreibt das Inspectorfeld Vorzeichen wie der Pitch Ruler (`Db`)?** Und **wie sieht eine negative Oktave aus** (`C -1`)?
27. **Zeigt das Feld bei Percussive/Universal einen Notennamen oder relative Halbtöne?** (Für den Ruler dokumentiert, fürs Feld nur erschlossen.)
28. **Was zeigen die Felder bei Mehrfachauswahl wirklich — ein Strich oder drei Bindestriche?** (Quellen widersprechen sich.)
29. **Gibt es das Leerzeichen in `C 3` auch auf Windows?**

### E. Pixel-Grundlagen
30. **Wie viele Appearance-Kontrasteinstellungen gibt es, wie heißen sie?** Und je Einstellung: **Referenzfarbe des Note-Editor-Hintergrunds im Edit- vs. NA-Modus** — das ist die primäre Modusprobe und muss pro Kontrast kalibriert sein.
31. **Welche DPI-Awareness meldet der Prozess** (System vs. Per-Monitor-v1)? Aus dem Prozess auslesbar, nicht aus dem Binary.
32. **Gibt es eine Mindestfenstergröße?** Weder Handbuch noch Systemvoraussetzungen nennen eine.

### F. Analyse
33. **Wie sieht der Detection-Fortschrittsindikator aus** — Form, Position, Text, Zahl? Ein Screenshot während des Ladens einer langen Datei beantwortet alles auf einmal.
34. **Welcher Cache-Pfad stimmt** (siehe §3.7)? Ordnerliste während einer laufenden Analyse — beantwortet zugleich, **ob ein Dateisystem-Watch als Fortschrittssignal taugt**.

### G. Optional, nicht ohne Freigabe
35. SIBIAC 0.23p2b18 (168 kB, GPLv2) herunterladen und `AppModules/sibiac/__init__.py` lesen — es ist die einzige existierende, wenn auch für Melodyne 4 kalibrierte, Koordinatentabelle.

---

# TEIL 3 — WO DIE RECHERCHE DEN AKTUELLEN ANNAHMEN WIDERSPRICHT

## 3.1 „Sechs Werkzeuge an festen Koordinaten"

**Halb richtig, in beiden Hälften.**

- **„Sechs" gilt nur im Edit-Modus.** [Q] Im **Note-Assignment-Modus enthält die Toolbox einen anderen Werkzeugsatz**, und *„which tools are available depends upon the algorithm"* (S. 78, 86, 90). [FIG] In den Abbildungen sind es dort **vier** Buttons. Ein Overlay mit sechs festen Slots liest im NA-Modus systematisch falsch — und der NA-Modus ist per `Strg+Shift+N` [BIN] und per Schraubenschlüssel-Icon einen Klick entfernt.
- **„Feste Koordinaten" ist durch mindestens sechs unabhängige Mechanismen widerlegt** (§9): das Info-Pane hat ~7 Positionen und verschiebt beide Kanten des Note Editors; Projekt-Tabs erscheinen erst ab dem zweiten Projekt; die Scale-Editor-Spalten sind dreistufig; das Fenster ist frei resizebar und die Top-Bar besteht aus 19 Zellen, die dabei umbrechen; die Windows-Skalierung skaliert alles (altes Shcore-Modell, keine v2-APIs); und die Buttongruppe links der Werkzeuge hat im Plug-in [FIG] **einen Button mehr** als im Stand-alone.
- **Zusatzelement:** [Q] Im NA-Modus erscheint **neben der Toolbox ein Slider** (S. 81) bzw. ein Slider mit zwei Indikatoren (S. 84) — die Werkzeugspalte ist dort nicht allein.
- **Und der Elternbutton ist kein festes Bitmap:** [FIG] er **rendert das Icon des gerade aktiven Untertools**. Ein Template „Pitch-Button" matcht nicht mehr, sobald Pitch Modulation aktiv ist.

**Konsequenz:** Ankern an der **linken Kante der Werkzeuggruppe**, nicht an der Fensterkante; Modus (Edit/NA) über die Hintergrundfarbe des Note Editors [Q, S. 74] vorschalten; DPI mit jedem Koordinatensatz speichern; das Layout per SIBIAC-Rezept festnageln und die Pane-Zustände zusätzlich **aus der Plist** verifizieren (§12) statt sie zu unterstellen.

## 3.2 „Die Varianten, die in Bereich `tools` gelistet sind"

**Die Liste ist unvollständig und die Schreibweisen weichen ab.** Richtig ist:

| Aktuell im Overlay | Befund |
|---|---|
| Pitch → „Pitch modulation" | richtig, korrekte Schreibweise **`Pitch Modulation Tool`** |
| Pitch → „Pitch drift" | richtig → **`Pitch Drift Tool`** |
| Amplitude → „Sibilant balance" | **unvollständig**: `Sibilant Balance Tool` ist das **zweite** Untertool (`F4`×3); das **erste** ist das **`Fade Tool`** (`F4`×2) — **fehlt** |
| Time → „Attack speed" | **unvollständig**: `Attack Speed Tool` ist `F5`×3; das **erste** ist das **`Time Handle Tool`** (`F5`×2) — **fehlt** |
| Note separation → „Separation Type" (geraten) | **Vermutung stimmt**: `Separation Type Tool`, 17 Handbuchtreffer, eigene Abschnittsüberschrift |
| *(fehlt)* | **Main → `Scroll Tool`, `Zoom Tool`** |
| *(fehlt)* | **NA-Modus ist ein eigener Satz**: dort hat die Separation-Gruppe **zwei** Untertools (Separation Type, dann **`Starting Point Tool`**), dazu **`Activation Tool`**, **`Sibilant Range Tool`**, **`Energy Share Tool`** |
| Basisname „Note separation" | Vollform **`Note Separation Tool`** |

**Formant hat als einziges Basiswerkzeug keine Variante** [Q + BIN] — und das ist auch pixelsichtbar (kein Dropdown-Dreieck) [FIG].

**Fade und Sibilant Balance sind Melodyne-5-Neuheiten** [Q] — im Melodyne-4-Handbuch 0 Treffer. Jede aus Melodyne-4-Vorarbeit übernommene Werkzeugliste ist deshalb zwangsläufig zu kurz.

**Schreibweisen, an denen sich das Overlay nicht festbeißen darf** [Q, alles Celemony-eigene Inkonsistenzen]: *Time Tool* (11×) vs. *Timing Tool* (5×, Seitentitel!), intern `Time Tools`; *Separation Type Tool* (17×) vs. *Note Separation Type Tool* (1×); *Energy Share Tool* (3×) vs. *Energy Assignment Tool* (1×); *Algorithm menu* vs. *Algorithms menu* vs. *Select Algorithm menu*; *toolbox* vs. *toolbar* vs. *tool bar* für dieselbe Werkzeugspalte — wobei „toolbar" **auch** die obere Leiste bezeichnet.

## 3.3 „Drücken-und-halten, dann ziehen" zur Variantenwahl

**Die Geste ist von Celemony nirgends dokumentiert, und der Ziehen-Teil von niemandem.**

- **[Q] Erschöpfende Suche über das komplette 280-Seiten-PDF und die Onlinereferenz:** `click and hold` = **0**, `clicking and holding` = **0**, `flyout` = **0**, `fly-out` = **0**, `popup` = **0**. `press and hold` kommt auf S. 147, 152, 161, 170 vor — **jedes Mal** über das Ziehen von Blobs oder das Halten von `[Alt]`/`[Cmd]`, **nie über die Toolbox**.
- Celemony sagt zu Untertools **nur**, wo sie liegen („beneath the Pitch Tool in the toolbar", „found below the Timing Tool") — plus F-Taste-Mehrfachdruck und Kontextmenü.
- **Die einzige Gestenaussage überhaupt kommt von Dritten**, beide **nicht** über Melodyne 5:
  - protoolstraining (älteres Melodyne, keine Versionsnummer auf der ganzen Seite): *„When you click and hold on the Main Tool button, you will see a drop-down menu with two additional tools, the Hand tool and Zoom tool."*
  - producerhive (Blog, unzuverlässige Nomenklatur — schreibt „Pitching tool", „Format tool"): *„To access the additional functions either left-click the icon and hold **or right-click**."*
- **Kein einziger Beleg für „dann ziehen".** Alle Quellen beschreiben ein **stehendes Dropdown-Menü**; die Celemony-Abbildungen zeigen den Mauszeiger **über dem Flyout** [FIG, `M5_ToolAmplitude_Fade_Sibilance_1.1`], was zu Klick-Menü-Klick genauso passt wie zu Halten-Ziehen.
- **Die Geste, die Celemony *tatsächlich* dokumentiert, betrifft ein anderes Widget** [Q]: *„If you click on the Clef icon (or the little arrow next to it) **while holding down the mouse key**, the menu containing the grid options drops down"* — Pitch Grid, nicht Toolbox. Analog für das Notenwert-Icon des Time Grid.

**Drei dokumentierte Alternativen, die die Geste komplett überflüssig machen:**
1. **F-Taste mehrfach** [Q] — kostet ein unbekanntes Zeitfenster.
2. **Kontextmenü des Note Editors** [Q] — **ausdrücklich auch für Untertools**, ohne Zeitfenster.
3. **Ein eigener Shortcut pro Untertool** [Q + BIN, 22 Direktbefehle] — ein Tastendruck, kein Menü, keine Koordinate, kein Timing. **Das ist der Weg, den dieses Dokument empfiehlt.**

## 3.4 „Anzeigefelder an festen Regionen, die Notenname und Cent zeigen"

**Das Modell stimmt nur für ein einziges Werkzeug.**

- **[FIG] Der Toolbar-Inspector ist *ein* Slot, der sich nur beim Pitch Tool auf zwei Felder aufspannt.** Pitch Modulation, Pitch Drift, Formant, Amplitude und Attack Speed zeigen **je ein Feld an derselben linken Position** (Innenmaß x286–378 im 750-px-Crop). Es gibt **kein** dauerhaftes „Feld 1 = Note, Feld 2 = Parameter".
  - **[KAL, 17.08.2026] Eingeschränkt:** das gilt für die fünf abgebildeten Werkzeuge, **nicht** fürs
    Main Tool — das zeigt zwei Felder (Beleg und Begründung in §6.1). Der Rest des Punktes steht.
- **[FIG] Die beiden Pitch-Felder sind gleich breit** (96 px außen, 12 px Abstand) — nicht etwa das zweite breiter.
- **Es ist nicht immer ein Notenname.** [Q] Bei **Percussive** und **Universal** kennt Melodyne nur relative Tonhöhen; bei **Mehrfachauswahl** steht ein **Strich**; im **NA-Modus sind die Pitch-Felder schreibgeschützt**; **[LÜCKE]** bei leerer Auswahl weiß niemand, was dort steht.
- **Es ist nicht immer Cent.** Je nach Werkzeug ist die Einheit `%` (Modulation, Drift, Attack Speed, Sibilant Balance), `dB` (Amplitude) oder `ct` (Cent, Formanten). `Hz` gibt es **nur** im Note Inspector.
- **Die Region ist nicht fest**: die Toolbar besteht aus 19 Zellen und bricht mit der Fensterbreite um; das Info-Pane (in dem der Note Inspector sitzt) hat ~7 Positionen; im Plug-in verschiebt eine zusätzliche Buttongruppe alles nach rechts.
- **Das Textformat weicht ab:** `C 3` **mit Leerzeichen**, `+4 ct` mit gedrucktem Plus, `83.0 %` **ohne** Plus, Einheit immer leerzeichengetrennt. Und die Namensschreibung ist per Preference auf **Deutsch (C, H, B)** oder **Latein (Do, Si, Sib)** umschaltbar.

**Empfehlung [Q]:** Der **Note Inspector** im Info-Pane ist das stabilere Ziel — er zeigt **alle** Parameter gleichzeitig, dauerhaft, unabhängig vom aktiven Werkzeug, und ist per Doppelklick **beschreibbar** (Zahl eintippen). Nachteil: das Info-Pane ist verschiebbar, seine Position muss also aus der Plist (`MDPrefsLeftInspectorPosKey`/`RightInspectorPosKey`) oder durch Festnageln bekannt sein. Im NA-Modus liefert er zusätzlich **Checkboxen** für „Hard separation" und „Starting Point" — les- und setzbar, ohne Doppelklick auf eine Trennungskoordinate.

## 3.5 „Klasse `GNWindowDoc`"

**Der String existiert und ist stabil — aber die Zuordnung ist ungeprüft, und sie reicht nicht allein.**

- **[BIN, bestätigt]** `GNWindowDoc` existiert als UTF-16-Klassenname, genau einmal, zusammen mit `GNWindow` und `GNWindowMenu`, unverändert in 5.3.1.018 → 5.4.0.036 → 5.4.1.004. `RegisterClassW`/`CreateWindowExW` sind importiert.
- **[INF, nicht verifiziert]** Dass ausgerechnet **`GNWindowDoc` das Hauptfenster** ist, ist eine Namens- und Nachbarschaftsschlussfolgerung. **Kein Live-Probe wurde gefahren.** Es kann genauso `GNWindow` sein.
- **Für das Plug-in gilt sie mit hoher Wahrscheinlichkeit nicht.** [BIN] Es gibt zusätzlich **`GNEmbedded%p`** — ein **pro Instanz mit Zeigersuffix erzeugter** Klassenname, im String-Cluster direkt neben `Window`, `Edit` und `tooltips_class32`. → **Auf `GNEmbedded` als Präfix matchen, niemals auf den vollen String**, und im DAW **beide** Kandidaten (`GNWindowDoc` und `GNWindow`) testen.
- **Klasse allein genügt nicht.** `GNWindow`/`GNWindowDoc` sind generische Toolkit-Namen; die Prozessidentität (`Melodyne.exe`, `ProductName=Melodyne`) muss dazu. **Nie auf den Fenstertitel matchen** — `SetWindowTextW` ist importiert, der Titel wird zur Laufzeit gesetzt und ist nirgends dokumentiert.

**Ein Zusatz, der die Erkennung erleichtert** [FORUM, azslow3 zu seinem eigenen Erkennungsproblem]: *„Most frameworks dynamically create Class names, so they are all JUCExxx or Pluginxxx. That is why I use ListView for detection."* — genau der Fall, den `GNEmbedded%p` hier darstellt.

## 3.6 Ein Widerspruch, den das Overlay noch nicht hat, aber bekommen wird

**Melodyne bietet unter Windows keinerlei UIA/MSAA für die Notenfläche** (§2.2, Nullbefund über 89,5 MB und die vollständige Importtabelle). Wenn das Overlay-Design an irgendeiner Stelle auf einen UIA-Baum hofft — für Werkzeugzustand, für Feldtexte, für Fokus —, ist das für Melodyne aussichtslos. **Frei nutzbar sind nur:** die **Menüleiste** (echtes Win32, per `GetMenu`/`WM_COMMAND` steuerbar, Alt öffnet sie aber nicht), die **Datei-Dialoge** (COMDLG32-Standarddialoge), und **Tab** als reine Framework-Navigation (keine Werksbelegung auf `Tab` [BIN], SIBIAC nutzt sie erfolgreich).

## 3.7 Widersprüche **zwischen den Quellen**, die er kennen muss

| Thema | Seite A | Seite B | Empfehlung |
|---|---|---|---|
| **Audio-Cache-Pfad** | PDF S. 58: `…\AppData\Local\com.celemony.melodyne\Separations`, *„predetermined and cannot be altered"* | `M5tour_InstallationActivation`: `C:\Users\*\Documents\Celemony\Separations`, Defender-Ausnahme dorthin | **Live prüfen** (Frage 34) — die Antwort entscheidet, ob ein Dateisystem-Watch als Analysesignal taugt |
| **`F5`-Untertool-Reihenfolge** | Melodyne 5 Handbuch S. 173/174: ×2 = Time Handle, ×3 = Attack Speed | SIBIAC-Forum (M4): *„F5 … one or 3 times. Pressing it **3 times** switch into unsupported **time handle** tool"*; SIBIAC-Wiki (M4): ×3 = Attack Speed | Handbuch für 5 nehmen; **empirisch prüfen** (Frage 11). Die beiden SIBIAC-Quellen widersprechen sich auch untereinander |
| **Mehrfachauswahl-Anzeige** | Pitch-Tool-Seite (en): *„three hyphens"* | Note Inspector, Modulation, Formant, Amplitude (en), Pitch-Tool (de), **zwei Abbildungen**: **ein Strich** | Ein Strich; Parser tolerant bauen |
| **Zoom auf Auswahl** | S. 45: `[Command]+[Shift]`+Doppelklick | S. 47 (Zusammenfassung, gleiches Kapitel): `[Command]+[Alt]`+Doppelklick | **Beide testen** (Frage 9), keine hart kodieren |
| **Separation-Type-Werkzeug: Klick oder Doppelklick?** | Trennungsseite und Amplitude-Seite: **Doppelklick** | Pitch-Seite: *„By **clicking** on a soft separation…"* | 2:1 für Doppelklick **[INF]** — oder ganz umgehen über die NA-Checkbox „Hard separation" |
| **`F5`-Taste im Handbuch** | Timing-Seite (online **und** PDF): *„pressing the **5** key … Press the **5** key twice or three times"* — CMS-Fehler, `<sup class="footnote">` frisst das „F", betrifft **zwei** Sätze | Time-Handles-Seite, Attack-Speed-Seite, Melodyne-4-Handbuch, Binary: **`F5`** | `F5` ist sicher; ob die blanke `5` **zusätzlich** wirkt: Frage 12 |
| **Shortcut-Kategorie für Trennungen** | Handbuch: *„Preferences → Shortcuts → Editing Tools → **Note Separation Tools**"* | Binary-Knotenlabel: **`Separation Tools`** | Auf den Binärnamen matchen |
| **Windows-Speicherpfad für Shortcut-Sätze** | Handbuch S. 63: `C:\ProgrammData\CelemonySoftwareGmbH\Shortcuts\Melodyne5` | Reale Platte: `C:\ProgramData\Celemony Software GmbH\` (Unterordner `Shortcuts` existiert noch nicht) | Handbuchpfad ist doppelt falsch |
| **SIBIAC und Melodyne 5** | azslow3, 04.07.2022: *„No, Melodyne 5 is not supported."* · Wiki (letzte Änderung **17.05.2023**): *„Supported version is melodyne 4."* | Changelog 29.03.2023: *„023p2b10 — NVDA 2023.1 support, **Melodyne 5 standalone**"* · LBL-Changelog 05.03.2023 (inzwischen gelöscht, nur im Archiv) | Stand-alone-Support kam im März 2023; Wiki und Forum-Handbuch sind nicht nachgezogen. **ARA wurde angekündigt, kein Beleg für Auslieferung** |
| **JAWS-Skripte und ARA** | Videobeschreibung (Autor, 12×): *„stand alone, ARA integration and the VST plugin"*, alle vier Editionen | Scott Chesworth, 11/2023: *„aren't compatible with Melodyne running in **REAPER** yet"* | Kein echter Widerspruch: die Bruchlinie ist **hostspezifisch**, nicht „ARA als Mechanismus". Erwarte Kontakt-artige Nested-Window-Probleme, und erwarte, dass der **Host** bricht, nicht ARA |
| **Fensterklassen-Inventar** | „Genau drei `GN…Window…`-Strings" | Zusätzlich `GNEmbedded%p`, `GNDragImage%p` | Kein Widerspruch — `GNEmbedded%p` enthält kein „Window" und fiel durch das Suchmuster. **Beide Befunde gelten** |
| **defkey / VI-Control** | Frühere Fassungen stützten sich darauf | Beide liefern jetzt **HTTP 403** | defkey nur noch aus lokaler Kopie belegt (7 Einträge, Melodyne **4**); die VI-Control-Behauptung ist **unverifizierbar** — nicht verwenden |

---

## Anhang — die drei Sätze, die alles andere aufwiegen

1. **Melodyne hat unter Windows keine Accessibility-API.** Für die Notenfläche ist Pixel/OCR nicht der bequeme Weg, sondern der einzige. [BIN, Nullbefund über vollständige Importtabelle]
2. **Fast jede Funktion ist einer eigenen Taste zuweisbar** — inklusive jedes einzelnen Untertools, per Direktbefehl im Action Pool. [Q + BIN] Das ersetzt Flyout-Geste, Doppeldruck-Zeitfenster und Werkzeugkoordinaten in einem Zug. Preis: einmalige Einrichtung, und die Shortcut-Seite ist ein Lernmodus, in dem **jede** gesendete Taste gebunden wird — inklusive Escape und Alt+F4.
3. **Der Fenster- und Pane-Zustand steht in einer Klartext-XML-Datei** (`%APPDATA%\Celemony Software GmbH\com.celemony.melodyne.plist`): Fensterrechteck, Sichtbarkeit jedes Panes, Inspector-Position. **Lesen statt messen.**