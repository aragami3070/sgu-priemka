import { ShieldCheck } from 'lucide-react'
import { Box, Typography } from '@mui/material'

interface BrandProps {
  compact?: boolean
}

export function Brand({ compact = false }: BrandProps) {
  return (
    <Box className="brand">
      <Box className="brand__mark" aria-hidden="true">
        <ShieldCheck size={compact ? 20 : 24} strokeWidth={1.8} />
      </Box>
      <Box>
        <Typography className="brand__name">СГУ</Typography>
        {!compact && (
          <Typography className="brand__caption">
            Управление учётными записями
          </Typography>
        )}
      </Box>
    </Box>
  )
}
