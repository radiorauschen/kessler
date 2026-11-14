# Änderungen

Was sich von einer Version zur nächsten geändert hat, neueste zuerst. Ein
Test prüft, dass die oberste Version die ist, die gebaut wird.

Eine Zeile, die mit `## ` beginnt, ist eine Version; eine, die mit `- `
beginnt, eine Änderung. Alles andere wird überlesen.

## 1.5.222

- **podcasts, warteschlange** wie bei musik: gespielte folgen bleiben blass darüber stehen, ein doppelklick springt vor oder zurück, eigener cursor für die tastatur (j/k, d, shift + j/k, zweimal shift + d leert, shift + pfeil markiert mehrere), jede folge nur einmal; eine folge, die noch lädt, bleibt an ihrem platz und sagt „lädt…"
- **ipod, experiment**: geladene, noch nicht gehörte folgen als playlist „podcasts" auf alte ipods; gehörte gehen beim nächsten abgleich wieder runter
- **ipod-abgleich** robuster: die datenbank wird ganz geschrieben, bevor sie die alte ersetzt; ein fehlender ordner lässt nur diesen titel scheitern
- **radio**: die zeile bleibt beim verbinden ruhig („sender verbinde…" mit laufenden punkten), wechsel werden überblendet, punkt und „live" gleiten; mit muto beginnt die animation mit einem quadrat, das am ende zum kreis wird
- **menüleiste (mac)**: schlaf-timer im menü, solange eine folge läuft (auch im linux-panel); cover über dem titel mit gleichem rand rundum
- **flüssiger**: kein kurzes ruckeln mehr alle 15 sekunden während einer folge; beim streamen einer folge seltener neu zeichnen; lautstärke angleichen misst mehrere platten zugleich
- **kleinigkeiten**: grosser weicher schatten am rechtsklick-menü; „bei last.fm lieben" im rechtsklick-menü; system-cursor am spaltentrenner; anzahl der künstler:innen oben rechts (menü → künstler:innen); pfeile laufen beim zurückwechseln nicht mehr nach; der rest eines mausrad-schritts geht nicht mehr in die nächste spalte; „windpark soft" unter „über" verlinkt
- **fehler behoben**: sehr grosse podcast-feeds liessen sich nicht laden; beim folgenwechsel stand kurz der musiktitel oben
- **messen**: zeigt den speicher jetzt und was die podcasts tun (feeds, ansichten, laden)

## 1.5.200

- **equalizer** (`z`): eine eigene spalte in musik, radio und podcasts — zehn bänder, voreinstellungen, zwei eigene; senkt automatisch so weit ab, dass nichts übersteuert
- **ansichten**: musik, radio, podcasts und ipod lassen sich einzeln ein- und ausschalten (menü → allgemein, oder `c` und ziffer); beim ersten start ist alles aus und das fenster sagt, wie man einschaltet; jede ansicht merkt sich ihre eigene seitenspalte
- **tastatur**: radio und podcasts ganz mit den pfeiltasten, das rechtsklick-menü mit `v`, cursor im suchfeld mit ← →, tastenübersicht (`?`) neu geordnet
- **podcasts**: feeds werden nur noch gefragt, ob sich etwas geändert hat; serien nach staffel und folge; schlaf-timer in der fusszeile; deutlich weniger arbeitsspeicher
- **radio**: beginnt ohne sender; ein sender ohne verbindung hält das fenster nicht mehr an und wird neu eingestellt
- **wiedergabe**: nach einer pause oder einem verlorenen ausgabegerät geht es an derselben stelle weiter; ein abgestürzter dekoder reisst nichts mehr mit
- **bilder**: beispielbild unter darstellung → bilder, das raster, töne und feinheit sofort zeigt; bilder im suchfeld gehen bis an den rand; kessler saga mit vorspann und mehr landschaft
- **mac**: zeichnet direkt über metal (weniger arbeitsspeicher), quadratisches symbol in der menüleiste, neues icon mit kante und schatten, .dmg mit eigenem bild, signieren und notarisieren per skript
- **linux**: mausrad und kopfzeile wie am mac, wahlweise vulkan (`KESSLER_GRAFIK=wgpu`)
- **menü**: „tasten", „neue version" und „sparsam" unter allgemein; wertung als sterne; zwischenablage des systems; neues standard-farbschema 公園, hell oder dunkel wie das system
- kessler sammelt nichts: keine telemetrie, keine analyse, keine absturzberichte — steht jetzt auch im readme und im handbuch

## 1.5.116

- **podcasts**: folgen über die feed-adresse oder die suche, neue folgen im hintergrund, laden, weiterhören wo man aufgehört hat, behalten, archivieren und löschen je folge, kapitel in der zeitleiste, eigene warteschlange (`w`), beschreibung und links der sendung (`i`), „offen" für noch nicht gehörtes, teilen, schlaf-timer bis zum ende der folge
- **radio**: stationen per stream-adresse, suche mit genre, land und bitrate, was als nächstes läuft, wo der sender es sagt, ruhigere leerzustände
- **menü** neu geordnet: eigene reiter allgemein, radio und podcasts; radio, podcasts und ipod lassen sich ganz abschalten; „einstellungen zurücksetzen"
- **kopfzeile**: neue umschalter zwischen musik, radio, podcasts und ipod, der menü-knopf in mehreren formen, alle bedienelemente nach einer farbregel
- **bilder**: neues raster „blaues rauschen", neu gewählte rasterfarben; kleinere bilder und ein begrenzter bildspeicher, deutlich weniger arbeitsspeicher
- **suchfeld und schoner**: kessler saga, kugel, zarathustra in misaki als laufender text, liedtext-schoner mit der gesungenen zeile in der mitte
- **wiedergabe**: rodio 0.23 — kein knacken mehr zwischen titeln mit verschiedenen abtastraten, keine verzerrten stimmen beim wechsel zwischen musik, radio und podcast, spulen ohne zu hängen
- **andere künstler:innen** als baum, linksbündig, klammern in der farbe ihrer namen
- menüleiste (mac) mit dem cover der laufenden folge; junicode und chivo mono sind wieder raus; viele kleinere fehler behoben

## 1.4.134

- die erste öffentliche fassung von kessler: musikplayer mit ipod-verwaltung für den mac und linux, mit radio, last.fm, liedtexten, informationen über künstler:innen und drei sprachen
