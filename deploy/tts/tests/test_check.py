import io
import wave
import pytest
from check import inspect_audio_stream


def wav(seconds=1):
    output=io.BytesIO()
    with wave.open(output,'wb') as audio:
        audio.setparams((1,2,24000,0,'NONE','not compressed'))
        audio.writeframes(b'\0\0'*int(24000*seconds))
    return io.BytesIO(output.getvalue())


def test_audio_inspection_uses_real_rate_and_requires_more_than_a_header():
    result=inspect_audio_stream(wav())
    assert result['sample_rate']==24000
    assert result['seconds']==1
    assert result['pcm_bytes']==48000


@pytest.mark.parametrize('seconds',[0,.3])
def test_empty_or_short_truncated_response_is_not_success(seconds):
    with pytest.raises(ValueError,match='short'):
        inspect_audio_stream(wav(seconds))
