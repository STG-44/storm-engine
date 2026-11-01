# SEA camera and frame boundaries

This increment retains CMatrix view construction / rigid Transposition and the
numeric SEA Realize boundary around the existing surface and weather controls.
It adds `--native-sea-stages`, requiring `--native-sea-weather` and its existing
dependencies. Earlier diagnostic modes retain their preceding timing and math.

## Retained behavior

CoreImpl::Run delivers script frame/events, then ProcessExecute, then
ProcessRealize. The new fleet mode delivers its bounded weather events before
ship movement/rocking. Ships query the retained surface from the preceding
Realize, including immediate parameter changes and any script-triggered
Realize(0). After those numeric ship calls, the sea receives Stage::Realize
through the existing Core EntityManager layer dispatcher. The bound sea actor
ignores Execute, as the original SEA::ProcessStage does.

SEA's retained numeric order is Stop gate, frame/movement update, camera
extraction and map preparation. Stop exits before camera access or preparation;
it holds the last sampling camera as the viewer moves. Parent-isDone callbacks
use the same bound view and zero-time path. A malformed provider or rejected
numeric update does not replace the published SEA state. This does not promise
transactional rollback of an entire frame or external script attribute writes.

The camera helpers retain complete BuildViewMatrix, SetInversePosition and
Transposition arithmetic, including degenerate fallbacks and the native rigid
transpose's treatment of all 16 elements. This is not a general matrix inverse.
The renderer builds the native view from its pinned orbit eye/target, reflects
renderer Z into native Z, and uses the corresponding reflected view for GPU
drawing. The GPU eye and sail-LOD eye come from that same transposed native view.
Native camera X/Z feed the scalar distance cutoff; the numeric camera and
world-offset assignment are reported. Original relative block geometry and
world-offset rendering remain unported.

The interactive viewer supplies its actual orbit parameters throughout initial
fixed-step replay and later frames. Follow-target coordinates are rebuilt at
each fixed step, keeping replay partition-independent for the same camera input.
A changed camera while paused requests zero-time preparation without a ship
step; repeated presentation of the same view is cached. This paused-viewer
adapter is distinct from running the complete original engine at delta zero.
Stop also blocks these camera refreshes; resume uses the latest bound view.

Staged model/effect presentation uses the already validated pinned preview
T*Ry*Rx*Rz math. This does not establish the complete native CMatrix accumulation
or native camera entities. The new report includes execute/render revisions and
query/render camera bits per frame; revisions are diagnostics, not native fields.

## Scope boundaries

The sea actor is dispatched by Core EntityManager in an explicit preview layer.
The ships execute their retained numeric body methods inside the Execute
boundary. Full original ship/sea registry, mounting, priorities/global layer
membership, camera entities and surrounding services are still integration work.
The report keeps `native_entity_order: false` for that broader unfinished scope.

Realize's debug controls, underwater tests, reflection/sunroad callbacks, texture
passes, block tree/LOD, SSE vertex generation and draw calls are not invoked by
this numeric path. Preview water retains its own shading/ray intersection. No
original game startup or naval combat loop is claimed. Resources remain external.

## Validation

The camera C++ oracle retains unedited methods from matrix.h: 64 complete
view/transpose profiles, including degenerate view directions and non-affine
transpose inputs. The stage oracle compiles the unedited Core execute/realize
loops and SEA stage switch around retained Realize stop/animation/camera blocks.
Layer membership, renderer input and query actors are explicit adapters. Its
32 sequences / 512 query-time and realized-state snapshots test old-state
queries, stopped camera retention, zero-time refresh and safe deletion. This is
not a complete SHIP::Execute or SEA rendering oracle. Both generators must
regenerate byte-identically.

Integration checks verify an actual rocking sample against the previous surface,
fixed-step partitioning, camera-only paused refresh, idempotent presentation,
Stop/resume camera retention, and equality of CPU/GPU/LOD camera eye inputs.
The existing 192-query GPU surface probe now obtains its camera through the new
Realize path, with a 0.0001 component tolerance. Original scalar and weather
oracles remain in the suite. Full land/gameplay/weather/rendering work remains
on the [marine checkpoint](MARINE_INTEGRATION.md); the whole-engine estimate
remains about 20% (15–25%).
