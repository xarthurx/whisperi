let audioCtx: AudioContext | null = null;

function getAudioContext(): AudioContext {
  if (!audioCtx) {
    audioCtx = new AudioContext();
  }
  return audioCtx;
}

function playTone(
  startFreq: number,
  endFreq: number,
  duration: number,
): void {
  try {
    const ctx = getAudioContext();
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();

    oscillator.connect(gain);
    gain.connect(ctx.destination);

    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(startFreq, ctx.currentTime);
    oscillator.frequency.linearRampToValueAtTime(endFreq, ctx.currentTime + duration);

    gain.gain.setValueAtTime(0.3, ctx.currentTime);
    gain.gain.linearRampToValueAtTime(0, ctx.currentTime + duration);

    oscillator.start(ctx.currentTime);
    oscillator.stop(ctx.currentTime + duration);
  } catch {
    // Audio not available, silently skip
  }
}

export function playStartSound(): void {
  playTone(600, 900, 0.15);
}

export function playStopSound(): void {
  playTone(900, 500, 0.2);
}
