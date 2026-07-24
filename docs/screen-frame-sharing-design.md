# Frame-Sharing für Screen-Primitiven (host.screen / host.ocr)

*Entscheidungsdokument, 2026-07-24. Anlass: Vorschlag, statt Live-Screen-Zugriffen pro Operation einen periodisch aktualisierten Screenshot des beobachteten Bereichs zu halten und alle Vergleichsoperationen dagegen auszuführen. Analyse: Call-Site-Inventar aller Module, Rust-Architektur-Audit, adversariale Design-Kritik (3 unabhängige Reviews + verifizierende Synthese), plus lokale GDI-Benchmarks.*

## 0. Messbasis

Benchmark auf der Referenzmaschine (GTX 1060, 1920×1080 @ 60 Hz, Windows 11; PowerShell-P/Invoke des identischen GDI-Pfads wie `WindowsBackend::capture`, Median aus 30 Läufen):

| Operation | Kosten |
|---|---|
| BitBlt-Capture 55×27 (Toggle-Region) | **16,6 ms** |
| BitBlt-Capture 633×418 (Kontakt-Content) | 17,0 ms |
| BitBlt-Capture 1028×666 (KK-Region) | 16,7 ms |
| BitBlt-Capture 1920×1080 (Vollbild) | 34,0 ms |
| Dasselbe mit wiederverwendetem DC+Bitmap | unverändert (±0,1 ms) |
| `GetPixel` | **16,7 ms pro Pixel** |

Interpretation: ~16,7 ms = exakt ein 60-Hz-Frame. Der DWM-Compositor serialisiert **jeden** GDI-Lesezugriff auf den Bildschirm auf seinen Vsync-Takt — die Kosten sind ein Fixblock pro *Berührung*, unabhängig von Regionsgröße und GDI-Setup. Konsequenz: Beschleunigung heißt **weniger Bildschirmberührungen**, nicht schnelleres Matching. (Genau das ist der gesunde Kern des Vorschlags.)

## 1. Antwort auf die Kernfrage

**Ja, das Konzept bringt Verbesserungen — aber nicht in der vorgeschlagenen Form.** Der richtige Kern der Idee ist: *ein* Capture, das *mehrere* Vergleichsoperationen bedient. Die falsche Hälfte ist die Zeitachse: ein **periodisch** aktualisierter Frame mit konfigurierbarem Intervall ist für ein Sprachausgabe-Tool die falsche Architektur.

- Teilen über **gleichzeitige Konsumenten** (beide gtoggle-Templates gegen ein Capture; N Library-Landmarks gegen ein Capture pro Recheck-Tick) ist korrekt, sicher und wird mit wachsender Library-Zahl sogar notwendig.
- Teilen über **Zeit** (periodischer Frame, Reads gegen einen bis zu T ms alten Puffer) erzeugt ein Staleness-Risiko genau dort, wo es am meisten schadet: Der 150-ms-Re-Read nach einem Toggle-Klick (overlay-runtime, `activate` gtoggle-Zweig) kann bei T > ~100 ms den **Vor-Klick-Zustand ansagen** — „Legato, aus", obwohl der Nutzer ihn gerade eingeschaltet hat. Ohne visuellen Kanal ist das ein stiller Vertrauensbruch. Ein sicheres T (≤ 50 ms) heißt dagegen 20 Hz BitBlt à 16,6 ms ≈ 33 % eines Kerns dauerhaft verbrannt, während der Nutzer einfach nur spielt — denn es gibt **keinen stehenden Konsumenten**: jeder Zustands-Read im System ist pull-basiert (Tab/Enter/Post-Klick).
- Die *fühlbare* Latenz liegt zudem gar nicht bei den Captures, sondern beim synchronen OCR statischer Labels (Abschnitt 2). Das Konzept attackiert Kosten unterhalb der Wahrnehmungsschwelle, während die hörbaren 50–200 ms woanders liegen.

Empfehlung: **Quick Wins (D) zuerst, dann Fan-out-Sharing pro Tick, DXGI (C2) als späterer Capture-Backend-Tausch. Das wörtliche Konzept C in keiner Form bauen.**

## 2. Wo die Kosten heute wirklich liegen

Nach Fühlbarkeit geordnet (alle Angaben gegen den Code verifiziert):

**Rang 1 — OCR bei Fokus (einziger Posten über der ~100-ms-Sprachbeginn-Wahrnehmungsschwelle).** `speakControl` ruft `host.ocr.recognize` synchron auf dem Mainthread (overlay-runtime; Host-Seite capture + WinRT inline). Kosten pro Tab-Stop: 17 ms Capture + 50–200 ms OCR. CSS hat 6 OCR-Buttons, davon lesen **5 (Violins 1/2, Violas, Cellos, Basses) ein Label, das sich nie ändert**; nur „Reverb" ist ein echter Live-Wert. Tab über die Sektionszeile ist die schlechteste gefühlte Latenz im ganzen System — und zu 100 % Verschwendung.

**Rang 2 — gtoggleState: zwei synchrone imageSearch auf derselben Region pro Tab.** Erst On-Template, dann Off-Template, identische Region, je ein eigenes Capture + ein eigener PNG-Decode von Platte (lib.rs `imageSearch`: `image::open` bei jedem Aufruf). Ist der Toggle aus, läuft das On-Template erst durch alle 10 `MATCH_SCALES` → ~34–40 ms Blockade pro Tastendruck. Korrektheits-Wart: die zwei Captures sind **zwei verschiedene Frames** ~17 ms auseinander — ein Umschalten mitten im Repaint kann beide Templates verfehlen (kein Zustand gesprochen).

**Rang 3 — Landmark-Poll: Dauerlast, die mit der Library-Zahl skaliert.** Jedes Library-Overlay registriert eigenes Gate + eigenen 500-ms-Poll; zusätzlich feuert jedes Fenster-Aktivieren/Fokus-Event ein Recheck. `imageSearchAsync` ist **nur halb asynchron** — Template-Decode UND Capture laufen auf dem Mainthread, nur der Pixelvergleich geht an den Worker (`ImageTask` trägt das fertige `cap`). Der Code-Kommentar „the capture itself is done here (cheap)" ist durch die Messung widerlegt. Steady State: ~3,4 % permanente Mainthread-Blockade **pro installierter Library**; mit 10 Libraries ≈ 34 % duty — plus 10× UIA-Identify pro Recheck (`cacheIdentity = false`, relational nötig).

**Rang 4 — Aktivierungs-Burst (begrenzt).** Alt-Tab in die DAW: Landmark-Recheck + What's-New-Scan (volle Region, auf 6 Versuche gedeckelt) + Fokus-Ansage nach 350 ms mit ggf. gtoggle/OCR = 2–4 Captures ≈ 34–68 ms, plus 50–200 ms wenn ein OCR-Control den Fokus hält.

**Gesamtszenario** (Tab durch das CSS-Overlay, ~19 Stops in 10 s): 0,7–1,6 s Mainthread-Blockade, konzentriert als 50–200-ms-Freezes exakt auf den Tastendrücken, bei denen Sprache prompt starten muss — blockiert wird dort alles: Speech, Hotkey-Dispatch, Timer, Arbiter-Rechecks.

**Nicht betroffen und schützenswert:** die AHK-artigen One-Shots — screen/ocr-Demos, der inspect-Hotkey, sforzandos Identity-OCR (per HWND gecacht), und die `host.screen.save`-Kalibrierhelfer, die *zwingend* den Live-Screen lesen müssen. Keine dieser Stellen profitiert von einem Frame-Provider; sie erzwingen, dass der Direkt-Capture-Pfad immer erhalten bleibt.

## 3. Bewertung der Varianten

**A — transparenter Kurz-TTL-Cache (30–50 ms, Invalidierung bei host.input.\*).** Sicher und nullkonfigurativ, *wenn*: TTL ≤ 50 ms (der 150-ms-Re-Read-Boden macht das beweisbar sicher), Invalidierung bei jedem synthetischen Input (architektonisch gratis: Input fließt durch denselben `Backend`-Trait — ein `CachingBackend`-Decorator um das `Rc<dyn Backend>` fängt beides ab), und **kein spekulatives Über-Capturen**. Ehrlich bewertet ist der Latenzgewinn **unhörbar** (33 → 17 ms); der echte Wert ist *Konsistenz* (beide Templates auf einem Frame). Aufwand M. AHK-kompatibel.

**B — explizites Snapshot-Handle (`grab(region)` → `{frame=}`).** Sauberste Korrektheits-Semantik: kohärente Reads per Konstruktion vom selben Frame; Staleness ist explizite Aufrufer-Entscheidung. Aber aktuell braucht es genau **eine** interne Stelle (gtoggleState) — dafür reicht ein minimales `imageSearchMulti` (ein Capture, mehrere Templates) ohne neue API-Fläche. B ist Ergänzung, nicht Fundament. Aufwand S–M. AHK-kompatibel (opt-in).

**C — periodischer Push-Provider (der wörtliche Vorschlag): abzulehnen.** (1) *Staleness vs. Sprachansage:* jedes billige Intervall kann nach dem Toggle-Klick den alten Zustand ansagen; jedes sichere Intervall (≤ 50 ms) verbrennt dauerhaft ~33 % eines Kerns in Compositor-Sync, in Konkurrenz zur Audio-Last der DAW. (2) *Der Konfigurations-Knopf ist per Konstruktion falsch:* der sichere Wert hängt von Interna ab (150-ms-Konstante, Plugin-Repaint-Latenz, MIDI-Keyswitch-Timing), die kein Nutzer kennen kann. Ein Regler, der „sagt falschen Zustand an" gegen „verbrennt Akku" tauscht, sollte nicht existieren. (3) *Kein Konsument:* nichts liest kontinuierlich; auch externe Änderungen (MIDI-Keyswitches, Host-Automation) werden nie spontan angesagt — Zustand wird nur bei Fokus gelesen. Ein Pull-Modell ist dafür automatisch korrekt. AHK-Constraint: **verletzt** (ein an eine konfigurierte Region gebundener Provider hilft nackten `pixel()`-Aufrufen nicht oder müsste den ganzen Screen schatten).

**C2 — DXGI Desktop Duplication: die einzig solide „Push"-Form, aber als letzter Schritt.** DXGI definiert Staleness weg: ein gehaltener Frame ohne neueren *ist* der aktuelle Bildschirm. Last ist änderungsgetrieben (nahe null bei statischem Bild; bei DAW-Playback Dirty-Rect-Filterung auf die beobachtete Region nötig). Einzigartiger Bonus: Dirty-Rects ermöglichen **ereignisgetriggerte Post-Toggle-Ansage** (ansagen, sobald die Toggle-Region tatsächlich neu gezeichnet wurde, statt fix 150 ms zu warten) — die einzige Frame-Provider-Verbesserung, die je hörbar würde. Kosten: Cargo.toml hat keine DXGI/D3D11-Features; per-Output (v1 Primärmonitor); DPI-Awareness prüfen; RDP/Secure-Desktop schlagen fehl → **BitBlt-Fallback bleibt für immer**; macOS braucht eine eigene Story (ScreenCaptureKit). Entscheidend: C2 passt **hinter die unveränderte `capture()`-Signatur** — kein API-Bruch, Schichten darüber bleiben gleich. Aufwand L. AHK-kompatibel, gerade *weil* es die Capture-Schicht ersetzt statt eine Watch-Region einzuführen.

**D — Mikro-Fixes: der eigentliche Gewinn.** Siehe Roadmap; D1 ist die einzige Änderung der gesamten Diskussion, die hörbar ist.

## 4. Priorisierte Roadmap

**Umsetzungsstatus (2026-07-24): Schritte 1–5 sind implementiert und verifiziert** (synthetische Selbsttests gegen den Live-Screen: Decode-Cache, `imageSearchMulti`-Index-Mapping, Worker-Thread-Capture und das Fan-out-Batching — vier gleichzeitige Async-Suchen, drei auf einer Region → ein Capture, alle Ergebnisse korrekt — bestanden alle Checks). Schritt 5 ist worker-seitig als 5-ms-Mikro-Batch mit Region-Dedup umgesetzt (transparent, kein API-Bruch). Schritt 6 ist durch 4/5 redundant; Schritt 7 (DXGI) bleibt das bewusste Später-Projekt.

| # | Maßnahme | Gewinn | Aufwand |
|---|---|---|---|
| 1 | **D1: Statische Labels nicht mehr OCRen.** Die 5 CSS-Sektions-OCRButtons auf statische Ansage umstellen — oder generisch: OCR-Ergebnis pro (Overlay, Control, Origin) cachen, nur Live-Werte (Reverb) frisch lesen. | **50–200 ms → ~0 pro Tab-Stop**; die einzige hörbare Verbesserung | S |
| 2 | **D3: Capture in den Image-Worker verlagern.** `ImageTask` trägt die *Region* statt des fertigen `cap`; die GDI-Sequenz (zustandslos) als freie, thread-sichere Funktion extrahieren, Worker captured selbst. Kommentar in lib.rs korrigieren. Kein Lua-API-Change. | Landmark-Poll + What's-New-Scan: ~17 ms/Tick Mainthread → 0; Idle-Floor 3,4 %/Library → ~0. Skelett für Schritt 5/7 | S–M |
| 3 | **D2: Template-Decode-Cache** (Pfad → RGBA, mtime-invalidiert) an beiden Decode-Stellen. Keine Staleness-Semantik (Dateien, nicht Screen). | Disk-I/O + PNG-Decode raus aus Poll und Toggle-Read; klein, gratis, bedingungslos korrekt | S |
| 4 | **Ein Capture für gtoggle** — `imageSearchMulti(region, templates)` (ein Capture, beide Matches); dazu in CSS `scales = {1.0}` für die selbst-capturten SwitchOn/Off-Templates. | ~17–20 ms pro Toggle-Fokus; behebt den Zwei-Frames-Wart; 10-Skalen-Leiter beim Off-Read entfällt | S |
| 5 | **Fan-out-Sharing pro Recheck-Tick:** ein Capture der Kontakt-Region pro Tick, geteilt von allen Landmark-Gates derselben Region (viele Templates, ein Frame). Der gesunde Kern des Konzepts — Sharing über *Konsumenten*, nicht über Zeit. | N Libraries = 1 statt N Captures pro Poll | M |
| 6 | *Optional* **A** als `CachingBackend`-Decorator (TTL ≤ 50 ms, Input-Invalidierung, nur exakt/enthaltene Regionen), falls 4/5 nicht einzeln gebaut werden. Rechtfertigung: Konsistenz, nicht Tempo. | Konsistenz + gelegentlich schnellere AHK-One-Shots | M |
| 7 | *Später* **C2: DXGI** hinter `capture()` im WindowsBackend, BitBlt-Fallback, v1 Primärmonitor, DPI-Awareness vorher prüfen. Bonus danach: Dirty-Rect-getriggerte Post-Toggle-Ansage statt der 150-ms-Heuristik. | Capture-Fixkosten strukturell weg; Post-Toggle-Ansage ~150 ms → ~Repaint | L |

Schritte 1–4 zusammen senken den schlimmsten Tastendruck von ~200 ms auf ~20 ms und den Idle-Floor auf ~0 — ohne Architekturänderung und ohne API-Bruch.

## 5. Was bewusst NICHT tun

1. **Kein periodischer Frame-Provider mit konfigurierbarem Intervall** (wörtliches Konzept C) — falsches Intervall = falsche Sprachansage ohne Korrekturkanal; richtiges Intervall = permanente CPU-Last ohne Konsumenten; der Knopf ist prinzipiell nicht sicher einstellbar.
2. **Das synchrone `imageSearch` nicht async machen** — die blockierende Semantik ist Teil des AHK-Versprechens; es wird über Schritt 4/6 billiger, nicht anders.
3. **Kein spekulatives Über-Capturen bei `pixel()`** — die „umschließende Region" eines ersten Pixel-Reads ist nicht erratbar; Vollbild (34 ms) für 8 Bytes wäre schlechter als heute.
4. **C2 nicht als ersten Schritt** — großer Lift, dessen Nutzen erst nach den D-Fixes den Aufwand rechtfertigt; und nie als einziger Pfad (BitBlt-Fallback ist Pflicht).
5. **Kein Caching über die Input-Grenze hinweg** — jede Cache-Variante invalidiert bei `host.input.click/key`; der Post-Klick-Re-Read darf nie einen Vor-Klick-Frame sehen. TTLs über ~100 ms sind kategorisch unsicher.
6. **Kalibrierpfade (`host.screen.save`, captureControl/captureAll) nie aus einem Cache bedienen** — sie erzeugen die Referenz-Templates und müssen den Live-Screen lesen. (Sinnvolle spätere Ausnahme: `captureAll` könnte per Variante B alle Crops aus *einem* Frame schneiden — für Konsistenz.)

**Fazit in einem Satz:** Der Instinkt „ein Frame, viele Vergleiche" ist richtig und wird gebraucht — aber als Sharing über gleichzeitige Konsumenten innerhalb eines Ticks (Schritt 5) und langfristig als DXGI-Backend (Schritt 7), nicht als periodischer Puffer; und das, was man beim Tabben tatsächlich *hört*, verschwindet schon vorher durch die vier kleinen D-Fixes — allen voran das Ende des OCR auf nie veränderliche Labels.
