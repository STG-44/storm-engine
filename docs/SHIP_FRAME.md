# Living SHIP frame and script state

`storm_sim::ship_presentation` retains the living frame boundary in original
SHIP::Execute. It snapshots vPos/vAng before Move, advances the body through
native movement/sea ports, then adds post-Move yaw rotation clamped to
-0.25..0.25 radians to the saved model roll. Heel is not multiplied by time
and never feeds physical State.vAng. The next rocking step therefore receives
physical angles without additive heel. Model position/yaw/pitch retain the
original one-Execute-call delay.

The publisher writes post-Move ship.speed, ship.pos and ship.ang in native
class/field order with retained %g float formatting. Speed.y is yaw rotation;
speed.z remains knots. X/Z offsets apply to script position; model immersion,
waterline, delay and heel do not. Missing classes/fields are created and unrelated
fields retained. Handle/numeric validation precedes writes; allocation failures
can retain earlier writes. advance_living rejects dead bodies before movement
or provider calls. Native sinking/dead Execute remains pending.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --ship-immersion 0.75 --ship-waterline 0.5 --time 3 \
  --particle-fleet-report ship-presentation-state.json
~~~

The native viewer enables this boundary automatically. Hold physical A/D to steer
the first ship. Key release, focus loss and restart centre its main turn input;
the second keeps independent diagnostic controls. The first starts centred.
Orbit/zoom, pause, immersion, fire and restart remain available. Steering sets
the diagnostic main rotation strength; speed/inertia limits still come from
explicit fixtures, not mounted sail/wind/cargo force configuration.

The opt-in batch flag adds presented-pose bits and nine script strings. Older
batch reports stay unchanged. Model/emitter histories use delayed, heeled roots
with native immersion/waterline correction. Body and script state remain
independent from model presentation. Repeated/fractional draws add no Execute
call. First-owner removal freezes its presentation/attributes; the second runs.

The oracle compiles five unedited movement/speed methods, native structs,
RotateAroundY, SetAttributeUseFloat, original Execute living/speed/position/angle
slices and the root expression with CVECTOR under UBSan, disabling FP contraction.
Forty-eight six-frame sequences compare all 288 complete snapshots: body,
strengths/acceleration, presented/root arguments, provider/math observations,
class creation and all nine script writes. Arithmetic replays native trig
observations exactly; pinned production kernels are independently checked within
one ULP with exact signs/zeros. Character tree/storage, numeric speed values,
construction, ShipRocking and CMatrix argument recording are adapters. The
independent 36-point rocking reference remains in the suite.

Cases cover heel clamps, signed zeros, zero/positive frames, impulses, control
reversals, fixed-speed/perk paths and headings. Further checks cover non-accumulation,
dead/invalid output rejection, delayed model versus current script values,
placed emitter histories, owner removal and fractional draws. This is standalone
scenario twenty-two.

Full Execute/Mount, native matrix construction, dead presentation, strand/contact
handling, original SEA/weather/wakes and original game-script integration remain
pending. This viewer is not complete commercial-game startup.
