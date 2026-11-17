program HelperBuiltinHidden;

// A declaration named like a built-in helper of `System` (objects.md 10.7) hides that helper with
// its members: `S.Length` and `I.ToString` are then the E3200 of a missing helper member, while
// the user's helper works and the built-in helpers of other types (`TByteHelper`) stay.

type
  TStringHelper = record helper for string
    function Twice: string;
  end;

  TIntegerHelper = record
    X: Integer;
  end;

function TStringHelper.Twice: string;
begin
  Result := Self + Self;
end;

var
  S: string;
  I: Integer;
  B: Byte;
begin
  S := 'ab';
  I := 3;
  B := 4;
  WriteLn(S.Twice, ' ', B.ToString);
  WriteLn(S.Length);    // E3200
  WriteLn(I.ToString);  // E3200
end.
