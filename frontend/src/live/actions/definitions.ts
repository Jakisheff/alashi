export const ACTION_SECONDS = 7.2
export type ScenarioActionName = 'mule' | 'bribe' | 'vote'
export type ScenarioPreview = {
  action: ScenarioActionName
  take: number
  playing: boolean
  speed: number
  entry: 'bottom' | 'side'
  seek: number | null
}
export type ActionStage = { at: number; label: string; text: string }

// Shared with geometry QA; framing changes do not alter action choreography.
export const scenarioFraming = (action: ScenarioActionName) => action === 'mule'
  ? { x: -.20, scale: .76 }
  : action === 'bribe' ? { x: -.12, scale: .82 } : { x: -.10, scale: .86 }

// Preview copy describes gestures, never asserts that a server action succeeded.
export const ACTION_META: Record<ScenarioActionName, {
  label: string; reducedFrame: number; stages: readonly ActionStage[]; keyframes: readonly number[]
}> = {
  mule: {
    label: 'Mule', reducedFrame: 3.65,
    stages: [
      { at: 0, label: 'Mule preview', text: 'Keep it quiet.' },
      { at: .8, label: 'Checking the coast', text: 'Anyone watching?' },
      { at: 1.6, label: 'Paying one peso', text: 'One peso.' },
      { at: 2.65, label: 'Receiving the parcel', text: 'One parcel. No questions.' },
      { at: 4.3, label: 'Keeping the goods', text: 'Mine now.' },
      { at: 5.7, label: 'Returning', text: 'You saw nothing.' },
      { at: 7.2, label: 'Preview complete', text: 'Ready for the next move.' },
    ], keyframes: [0, .8, 1.4, 1.9, 2.15, 2.45, 2.7, 2.95, 3.2, 3.45, 3.65, 4.3, 4.9, 5.4, 6.2, 7.2],
  },
  bribe: {
    label: 'Bribe', reducedFrame: 3.7,
    stages: [
      { at: 0, label: 'Bribe preview', text: 'A private matter.' },
      { at: .8, label: 'At the official’s desk', text: 'A moment of your time?' },
      { at: 1.6, label: 'Presenting the envelope', text: 'For your consideration.' },
      { at: 3.1, label: 'Discreet exchange', text: 'Between us.' },
      { at: 4.65, label: 'Envelope received', text: 'No further questions.' },
      { at: 5.7, label: 'Returning', text: 'Back to business.' },
      { at: 7.2, label: 'Preview complete', text: 'Ready for the next move.' },
    ], keyframes: [0, .8, 1.35, 2.0, 2.7, 3.1, 3.65, 3.85, 4.05, 4.35, 4.65, 5.2, 5.4, 6.2, 7.2],
  },
  vote: {
    label: 'Vote', reducedFrame: 3.3,
    stages: [
      { at: 0, label: 'Vote preview', text: 'Time to make a choice.' },
      { at: .8, label: 'Lifting the ballot', text: 'One ballot. One voice.' },
      { at: 2.15, label: 'Above the ballot box', text: 'Make it count.' },
      { at: 3.35, label: 'Dropping the ballot', text: 'Here goes.' },
      { at: 4.35, label: 'Ballot inside', text: 'Let the count begin.' },
      { at: 5.7, label: 'Returning', text: 'Your move.' },
      { at: 7.2, label: 'Preview complete', text: 'Ready for the next move.' },
    ], keyframes: [0, .8, 1.5, 2.15, 2.8, 3.35, 3.5, 3.75, 4.1, 4.35, 5.3, 6.2, 7.2],
  },
}

export function actionStageAt(action: ScenarioActionName, time: number): ActionStage {
  const stages = ACTION_META[action].stages
  return stages.findLast((stage) => time >= stage.at) ?? stages[0]
}

export const smooth = (value: number) => {
  const x = Math.max(0, Math.min(1, value))
  return x * x * (3 - 2 * x)
}
export const ramp = (time: number, from: number, to: number) => smooth((time - from) / (to - from))
export const clampTime = (time: number) => Number.isFinite(time) ? Math.max(0, Math.min(ACTION_SECONDS, time)) : 0
