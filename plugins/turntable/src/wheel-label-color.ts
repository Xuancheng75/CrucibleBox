export function wheelLabelColorFor(backgroundColor: string, fallback: string): string {
  const match = /^#([\da-f]{3}|[\da-f]{6})$/i.exec(backgroundColor.trim())
  if (!match) return fallback

  const shorthand = match[1].length === 3
  const hex = shorthand ? [...match[1]].map((digit) => digit + digit).join('') : match[1]
  const channels = [0, 2, 4].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16) / 255)
  const linear = channels.map((channel) =>
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  )
  const luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]

  return (luminance + 0.05) / 0.05 > 1.05 / (luminance + 0.05) ? '#000' : '#fff'
}
