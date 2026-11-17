program DumpDefaultProperties;

// The dumps of default array properties (objects.md 15.6, 12.4): `.property … default` in the
// type, a read as the call of the `read` method and a write as the `property` target like any
// indexed property, a range as `range-index` with the bounds written (a new `TIndexRange`, type
// index 26, made in the bytecode by `Unique` and `FieldSet`), and `Clamp` written out as
// comparisons on hidden variables.

type
  TSeq = class(TRefObject)
    function Get(I: Integer): Integer;
    procedure Put(I: Integer; V: Integer);
    function Count(const R: TIndexRange): Integer;
    property Items[I: Integer]: Integer read Get write Put; default;
    property Items[const R: TIndexRange]: Integer read Count; default;
  end;

function TSeq.Get(I: Integer): Integer;
begin
  Result := I;
end;

procedure TSeq.Put(I: Integer; V: Integer);
begin
end;

function TSeq.Count(const R: TIndexRange): Integer;
var
  F, L: Integer;
begin
  R.Clamp(0, 9, F, L);
  Result := L - F + 1;
end;

var
  S: TSeq;
begin
  S := TSeq.Create;
  S[1] := S[2];
  WriteLn(S[3..], S[..4]);
end.
