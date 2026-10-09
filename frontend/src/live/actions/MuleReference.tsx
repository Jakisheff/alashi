import { useTexture } from '@react-three/drei'
import { useEffect } from 'react'
import { SRGBColorSpace } from 'three'
import type { ScenarioProps } from './props'

const COURIER = `${import.meta.env.BASE_URL}images/live/mule-courier-v1.webp`

/** Mounted only for Mule. Drei owns this cached texture; props own the material. */
export function MuleReference({ props }: { props: ScenarioProps }) {
  const texture = useTexture(COURIER, (loaded) => { loaded.colorSpace = SRGBColorSpace })
  useEffect(() => {
    props.setDonkeyTexture(texture)
    return () => props.setDonkeyTexture(null)
  }, [props, texture])
  return null
}
