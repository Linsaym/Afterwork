import actions from './ipc-actions.json' with { type: 'json' }
import snapshot from './ipc-snapshot.json' with { type: 'json' }
import updateStatus from './ipc-update-status.json' with { type: 'json' }
import type { UpdateStatus } from '../src/desktop'
import type { Action, Snapshot } from '../src/types'

// JSON imports infer plain strings. The Rust round-trip test checks the wire discriminants.
type JsonShape<T> = T extends string ? string : T extends object ? { [K in keyof T]: JsonShape<T[K]> } : T

export const actionFixtures: JsonShape<Action>[] = actions
export const snapshotFixture: JsonShape<Snapshot> = snapshot
export const updateStatusFixture: JsonShape<UpdateStatus> = updateStatus
