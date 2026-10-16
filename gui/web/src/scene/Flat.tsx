// The flat scene (plan §4.6, `view.flat`, or no WebGL): the same model as
// a list — one text cat ᓚᘏᗢ per session with the TUI's pill colours, its
// bubble, collar and kittens. Everything stays clickable.

import type { CatModel, Pose, SceneModel } from './model.ts';
import { stateToken } from '../state/theme.ts';

export const POSE_TEXT: Record<Pose, string> = {
  typing: '⌨',
  alert: '!',
  sleeping: 'zZ',
  sit: '',
  puff: '✶',
};

/** The text line of a cat, e.g. "ᓚᘏᗢ !" or "ᓚᘏᗢ zZ ✉". */
export function flatCatText(c: CatModel): string {
  const parts = ['ᓚᘏᗢ', POSE_TEXT[c.pose]];
  if (c.mail) parts.push('✉');
  return parts.filter(Boolean).join(' ');
}

export function FlatScene({
  model,
  onOpenSession,
  reason,
}: {
  model: SceneModel;
  onOpenSession(id: string): void;
  reason: 'flat' | 'webgl';
}) {
  return (
    <div class="nk-scene nk-scene-flat" data-testid="scene" data-mode="flat">
      {reason === 'webgl' && <p class="nk-flat-note">sem WebGL — cena em modo 2D</p>}
      {model.stations.map((st) => (
        <section key={st.ctx} class="nk-flat-station" style={{ '--st': st.color }}>
          <h3>{st.ctx || '·'}</h3>
          <ul>
            {model.cats
              .filter((c) => c.ctx === st.ctx)
              .map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    class={`nk-flat-cat pose-${c.pose}${c.dim ? ' dim' : ''}${c.mail?.blink ? ' mail' : ''}`}
                    data-session={c.id}
                    data-pose={c.pose}
                    onClick={() => onOpenSession(c.id)}
                    title={c.label}
                  >
                    <span class="cat">{flatCatText(c)}</span>
                    <span class="label">{c.label}</span>
                    <span class="pill" style={{ '--pill': `var(--nk-${stateToken(c.state)})` }}>
                      {c.state}
                    </span>
                    {c.collar && <span class={c.collarLit ? 'collar lit' : 'collar'} style={{ '--collar': c.collar }} />}
                    {c.kittens.length > 0 && <span class="kittens">{`ᓚᘏᗢ ×${c.kittens.length}`}</span>}
                  </button>
                </li>
              ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
