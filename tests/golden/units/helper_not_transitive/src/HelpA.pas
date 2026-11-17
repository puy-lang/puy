unit HelpA;

// Declares a helper for `string` in its `interface` section.

interface

type
  TBangHelper = record helper for string
    function Bang: string;
  end;

implementation

function TBangHelper.Bang: string;
begin
  Result := Self + '!';
end;

end.
