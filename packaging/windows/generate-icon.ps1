$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing

$size = 256
$bitmap = New-Object System.Drawing.Bitmap $size, $size
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.Clear([System.Drawing.Color]::FromArgb(0, 0, 0, 0))

$background = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 34, 40, 49))
$border = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 57, 62, 70), 16)
$accent = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 0, 173, 181))
$textBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 238, 238, 238))

$rect = New-Object System.Drawing.Rectangle 16, 16, 224, 224
$graphics.FillRectangle($background, $rect)
$graphics.DrawRectangle($border, $rect)
$graphics.FillRectangle($accent, 64, 64, 128, 128)

$font = New-Object System.Drawing.Font('Segoe UI Semibold', 72, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
$format = New-Object System.Drawing.StringFormat
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center
$graphics.DrawString('P', $font, $textBrush, [System.Drawing.RectangleF]::new(0, 0, $size, $size), $format)

$pngStream = New-Object System.IO.MemoryStream
$bitmap.Save($pngStream, [System.Drawing.Imaging.ImageFormat]::Png)
$pngBytes = $pngStream.ToArray()

$iconPath = Join-Path $PSScriptRoot 'pole.ico'
$fileStream = [System.IO.File]::Create($iconPath)
$writer = New-Object System.IO.BinaryWriter($fileStream)

$writer.Write([UInt16]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]1)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]32)
$writer.Write([UInt32]$pngBytes.Length)
$writer.Write([UInt32]22)
$writer.Write($pngBytes)
$writer.Flush()
$writer.Close()

$graphics.Dispose()
$bitmap.Dispose()
$background.Dispose()
$border.Dispose()
$accent.Dispose()
$textBrush.Dispose()
$font.Dispose()
$pngStream.Dispose()

Write-Host "Generated icon: $iconPath"
