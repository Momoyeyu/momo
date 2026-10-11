  const fit = Math.min(naturalWidth, Math.max(1, viewportWidth))
  return (mode === 'fit' ? fit : Math.max(fit, naturalWidth * 14 / 15)) * zoom
}
