"""Copy source snapshots and generate narrowly scoped diagnostic variants."""
from pathlib import Path
import shutil

HERE = Path(__file__).resolve().parent
SOURCE = Path('C:/code/aethos/experiments')
(HERE / 'src').mkdir(exist_ok=True)
for source in (SOURCE / 'src').glob('*.rs'):
    shutil.copyfile(source, HERE / 'src' / source.name)
shutil.copyfile(SOURCE / 'Cargo.lock', HERE / 'Cargo.lock')
manifest = (SOURCE / 'Cargo.toml').read_text().split('[[bench]]')[0]
manifest = manifest.replace('name = "ecs-transform-experiments"', 'name = "aethos-transform-diagnosis"')
manifest = manifest.replace('path = "../code/util"', 'path = "C:/code/aethos/code/util"')
manifest = manifest.replace('path = "../../edict"', 'path = "C:/code/edict"')
manifest += '\n[features]\nphases = []\n'
(HERE / 'Cargo.toml').write_text(manifest)
original = (SOURCE / 'src/edict_bevy_backend.rs').read_text()
original = original.replace('struct Global(Transform);', 'pub(crate) struct Global(pub(crate) Transform);')
original = original.replace('struct LocalPosition(LocalTransform);', 'pub(crate) struct LocalPosition(pub(crate) LocalTransform);')
original = original.replace('struct Marks {\n    dirty: u64,\n    covered: u64,\n}', 'pub(crate) struct Marks {\n    pub(crate) dirty: u64,\n    pub(crate) covered: u64,\n}')
(HERE / 'src/edict_bevy_backend.rs').write_text(original)
shared_components = original[original.index('#[derive(Component)]'):original.index('#[derive(Default)]')]
alt = original.replace('query::{Entities,', 'query::{Alt, Entities,')
alt = alt.replace('View<&mut Marks>', 'View<Alt<Marks>>').replace('View<&mut Global>', 'View<Alt<Global>>').replace('ViewCell<&mut Global>', 'ViewCell<Alt<Global>>')
alt = alt.replace('let Ok(marker) =', 'let Ok(mut marker) =').replace('let Ok(global) = globals.try_get_mut(entity)', 'let Ok(mut global) = globals.try_get_mut(entity)')
narrow = original.replace("type Hierarchy<'w>", "type CoverHierarchy<'w> = View<'w, Option<Related<With<ChildOf>>>>;\n\ntype Hierarchy<'w>")
start, end = narrow.index('fn cover('), narrow.index('fn update(')
part = narrow[start:end].replace('hierarchy: &Hierarchy', 'hierarchy: &CoverHierarchy').replace('Ok((_, Some(children), _))', 'Ok(Some(children))')
narrow = narrow[:start] + part + narrow[end:]
narrow = narrow.replace('    hierarchy: Hierarchy,', '    hierarchy: Hierarchy,\n    cover_hierarchy: CoverHierarchy,')
narrow = narrow.replace('mark(entity, pass, &mut markers, &hierarchy)', 'mark(entity, pass, &mut markers, &cover_hierarchy)')
narrow = narrow.replace('mark(entity.id(), pass, &mut markers, &hierarchy)', 'mark(entity.id(), pass, &mut markers, &cover_hierarchy)')
located = original.replace('View<&mut Marks>', 'View<(Entities, &mut Marks)>')
located = located.replace('        Option<RelatesExclusive<With<ChildOf>>>,', '        Entities,\n        Option<RelatesExclusive<With<ChildOf>>>,')
located = located.replace('let Ok(marker) = markers.try_get_mut(entity)', 'let Ok((loc, marker)) = markers.try_get_mut(entity)')
located = located.replace('Ok((_, Some(children), _)) = hierarchy.try_get(entity)', 'Ok((_, _, Some(children), _)) = hierarchy.try_get(loc)')
located = located.replace('Ok((_, children, Some(local))) = hierarchy.try_get(entity)', 'Ok((loc, _, children, Some(local))) = hierarchy.try_get(entity)')
located = located.replace('Ok((parent, children, local)) = hierarchy.try_get(entity)', 'Ok((loc, parent, children, local)) = hierarchy.try_get(entity)')
located = located.replace('globals.try_get_mut(entity)', 'globals.try_get_mut(loc)')
located = located.replace('.is_ok_and(|marker| marker.', '.is_ok_and(|(_, marker)| marker.')
for name, source in [('edict_alt_backend', alt), ('edict_narrow_backend', narrow), ('edict_located_backend', located)]:
    source = source.replace(shared_components, 'use crate::edict_bevy_backend::{Global, LocalPosition, Marks};\n\n')
    source = source.replace('    component::Component,\n', '')
    (HERE / 'src' / f'{name}.rs').write_text(source)
    with (HERE / 'src/lib.rs').open('a') as f:
        f.write(f'\npub mod {name};\n')

# Instrumented copies retain the source algorithm. Timers run only at phase boundaries.
for name in ['edict_bevy_backend', 'bevy_backend', 'edict_alt_backend', 'edict_narrow_backend', 'edict_located_backend']:
    path = HERE / 'src' / f'{name}.rs'
    source = path.read_text()
    source = source.replace('    scratch.pass = scratch.pass.checked_add', '    #[cfg(feature = "phases")]\n    let phase_start = std::time::Instant::now();\n    scratch.pass = scratch.pass.checked_add')
    boundary = '    let mut globals = globals.lock();' if name != 'bevy_backend' else '    let mut globals = globals.p1();'
    source = source.replace(boundary, '    #[cfg(feature = "phases")]\n    let marked = phase_start.elapsed();\n' + boundary)
    start = source.index('fn sync_global(')
    end = source.index('\n}\n', start)
    source = source[:end] + '\n    #[cfg(feature = "phases")]\n    {\n        MARK_NS.store(marked.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);\n        UPDATE_NS.store((phase_start.elapsed() - marked).as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);\n    }' + source[end:]
    source += '\n#[cfg(feature = "phases")]\nstatic MARK_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\n#[cfg(feature = "phases")]\nstatic UPDATE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\npub fn phases() -> (u64, u64) {\n    #[cfg(feature = "phases")]\n    return (MARK_NS.load(std::sync::atomic::Ordering::Relaxed), UPDATE_NS.load(std::sync::atomic::Ordering::Relaxed));\n    #[cfg(not(feature = "phases"))]\n    (0, 0)\n}\n'
    path.write_text(source)

source = (HERE / 'src/edict_3_backend.rs').read_text()
source += '''
pub fn idle_writes(count: usize) -> [usize; 3] {
    let mut backend = Adapter::new(&crate::forest(count));
    backend.sync();
    std::array::from_fn(|_| {
        let epoch = backend.world.epoch();
        backend.sync();
        backend.world.view::<Entities>()
            .filter(Modified::<With<Global>>::new(epoch)).iter().count()
    })
}
'''
(HERE / 'src/edict_3_backend.rs').write_text(source)
source = source.replace('query::{Cpy,', 'query::{Alt, Cpy,')
source = source.replace('View<&mut Global>', 'View<Alt<Global>>')
source = source.replace('globals.get_mut(parent).copied()', 'globals.get_mut(parent).map(|global| *global)')
source = source.replace('globals.get_mut(e).copied()', 'globals.get_mut(e).map(|global| *global)')
source = source.replace('let Some(global) = globals.get_mut(e) else', 'let Some(mut global) = globals.get_mut(e) else')
(HERE / 'src/edict_3_alt_backend.rs').write_text(source)
with (HERE / 'src/lib.rs').open('a') as f:
    f.write('\npub mod edict_3_alt_backend;\n')
