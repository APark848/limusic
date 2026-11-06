"""Generate owned, silent PCM media for Apple's AAC encoder; no external content."""
import struct
import sys

size = 7200 * 8000 * 2  # two hours, mono 8 kHz signed 16-bit PCM
header = struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', size + 36, b'WAVE', b'fmt ',
                     16, 1, 1, 8000, 16000, 2, 16, b'data', size)
with open(sys.argv[1], 'wb') as output:
    output.write(header)
    output.truncate(44 + size)
