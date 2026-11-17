unit Shouting;

interface

type
  TShoutHelper = record helper for string
    function Shout: string;
  end;

function Quiet(const S: string): string;

implementation

type
  // Declared in `implementation`: not visible to the units that use this one
  TWhisperHelper = record helper for string
    function Whisper: string;
  end;

function TShoutHelper.Shout: string;
begin
  Result := Self + '!';
end;

function TWhisperHelper.Whisper: string;
begin
  Result := '(' + Self + ')';
end;

function Quiet(const S: string): string;
begin
  Result := S.Whisper;
end;

end.
