/**
 * The ambient orbital field behind the hero: concentric tilted orbits, each
 * carrying a single travelling node, over a soft brand core, a faint nebula,
 * and a sparse starfield.
 *
 * Purely decorative — CSS only, no JavaScript and no canvas — so it costs no
 * main-thread time and never blocks interaction. The global reduced-motion
 * rules in `globals.css` freeze the spin and reveal for visitors who ask for
 * less motion.
 */

const ORBITS = [
  { size: 116, tilt: 60, skew: 0, spin: 46, delay: 0, node: true },
  { size: 88, tilt: 68, skew: 24, spin: 34, delay: 10, node: false },
  { size: 60, tilt: 64, skew: -18, spin: 58, delay: 20, node: true },
  { size: 38, tilt: 72, skew: 10, spin: 40, delay: 6, node: false },
] as const;

export function OrbitField({ className = "" }: { className?: string }) {
  return (
    <div aria-hidden className={`orbit-field ${className}`}>
      <div className="orbit-nebula" />
      <div className="orbit-stars" />
      <div className="orbit-scene">
        {ORBITS.map((orbit, i) => (
          <div
            key={i}
            className="orbit-plane"
            style={{
              transform: `rotateX(${orbit.tilt}deg) rotateZ(${orbit.skew}deg)`,
            }}
          >
            <div
              className="orbit-ring"
              style={{
                width: `${orbit.size}%`,
                height: `${orbit.size}%`,
                animationDuration: `${orbit.spin}s`,
                animationDelay: `-${orbit.delay}s`,
              }}
            >
              {orbit.node ? <span className="orbit-node" /> : null}
            </div>
          </div>
        ))}
        <span className="orbit-core" />
      </div>
    </div>
  );
}
