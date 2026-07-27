-- Plays a bundled audio asset via host.sound (fire-and-forget).

host.log.info("Playing a bundled sound asset...")
host.sound.play("assets/ding.wav")
host.speech.output("Played a sound. Did you hear the ding?", { interrupt = true })
