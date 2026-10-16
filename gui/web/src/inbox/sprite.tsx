// The inbox cat: a tiny oneko-style pixel cat drawn in code (no assets).
// Black body, yellow eyes (theme tokens cat_body / cat_eyes), facing right.
// Four walk frames swap by a CSS steps() animation that only exists while
// the walker walks; arrived, it stands still in a single static frame.

/** Rows shared by every frame: '#' body, 'e' eye, '.' empty. */
const BODY = [
  '...........#....#.',
  '#..........##..##.',
  '#..........######.',
  '#..........#e##e#.',
  '.#.........######.',
  '.#..........####..',
  '..###########.....',
  '..############....',
  '..############....',
  '...##########.....',
];

/** The two leg rows of each pose. */
export const LEGS = {
  stand: ['...#.#.....#.#....', '...#.#.....#.#....'],
  walk: [
    ['..#..#....#..#....', '.#....#..#....#...'],
    ['...##......##.....', '...##......##.....'],
    ['...#.#....#.#.....', '....#.#....#.#....'],
    ['....##.....##.....', '....##.....##.....'],
  ],
} as const;

export const SPRITE_W = 18;
export const SPRITE_H = BODY.length + 2;
/** On-screen size of one pixel (integer: crisp edges). */
export const PX = 3;

export interface Run {
  x: number;
  y: number;
  w: number;
  eye: boolean;
}

/** Pixel rows as horizontal runs (one <rect> each), starting at row y0. */
export function runs(rows: readonly string[], y0 = 0): Run[] {
  const out: Run[] = [];
  rows.forEach((row, dy) => {
    let i = 0;
    while (i < row.length) {
      const c = row[i]!;
      if (c === '.') {
        i++;
        continue;
      }
      let j = i;
      while (j < row.length && row[j] === c) j++;
      out.push({ x: i, y: y0 + dy, w: j - i, eye: c === 'e' });
      i = j;
    }
  });
  return out;
}

const BODY_RUNS = runs(BODY);
const STAND_RUNS = runs(LEGS.stand, BODY.length);
const WALK_RUNS = LEGS.walk.map((f) => runs(f, BODY.length));

function Rects({ rs }: { rs: readonly Run[] }) {
  return (
    <>
      {rs.map((r) => (
        <rect key={`${r.x},${r.y}`} x={r.x} y={r.y} width={r.w} height={1} class={r.eye ? 'nk-px-eye' : 'nk-px'} />
      ))}
    </>
  );
}

/** The cat; `walking` adds the four leg frames, `left` mirrors it. */
export function CatSprite({ walking, left }: { walking: boolean; left: boolean }) {
  return (
    <svg
      class={`nk-cat${left ? ' nk-cat-left' : ''}`}
      viewBox={`0 0 ${SPRITE_W} ${SPRITE_H}`}
      width={SPRITE_W * PX}
      height={SPRITE_H * PX}
      shape-rendering="crispEdges"
      aria-hidden="true"
    >
      <Rects rs={BODY_RUNS} />
      {walking ? (
        WALK_RUNS.map((rs, i) => (
          <g key={i} class={`nk-frame nk-frame-${i}`}>
            <Rects rs={rs} />
          </g>
        ))
      ) : (
        <Rects rs={STAND_RUNS} />
      )}
    </svg>
  );
}
