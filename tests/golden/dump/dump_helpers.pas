program DumpHelpers;

// The dumps of helpers (objects.md 15.6): `.type … = record helper for …` with the parent and
// the members; `helper-call` for a helper's method called on a value and as `H.M(X)`; the
// built-in `ToString` / `Length` as `builtin` lines; the functions named after the helper.

type
  TCounter = class
  public
    N: Integer;
  end;

  TCounterHelper = class helper for TCounter
    procedure Bump;
    class function Make: TCounter;
  end;

  TTextHelper = record helper for string
    function Twice: string;
  end;

  TTextMore = record helper(TTextHelper) for string
  public
    class var Calls: Integer;
    function Size: Integer;
    property Len: Integer read Size;
  end;

procedure TCounterHelper.Bump;
begin
  N := N + 1;
end;

class function TCounterHelper.Make: TCounter;
begin
  Result := TCounter.Create;
end;

function TTextHelper.Twice: string;
begin
  Result := Self + Self;
end;

function TTextMore.Size: Integer;
begin
  Calls := Calls + 1;
  Result := Length(Self);
end;

var
  C: TCounter;
  S: string;
  I: Integer;
begin
  C := TCounter.Make;
  C.Bump;
  TCounterHelper.Bump(C);
  S := 'ab';
  WriteLn(S.Twice, S.Len, C.N.ToString, S.Length);
  I := 3;
  WriteLn(I.ToString);
  C.Free;
end.
