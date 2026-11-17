program HintScopes;

// Where a use is not reported (attributes.md 6.2): inside the marked declaration itself (a
// recursive call, the methods of a marked class using its name), inside a declaration with the
// same mark (a deprecated routine using another one; an experimental one still warns about a
// deprecated one), in the heading of a body and in an `override` (the call through the parent's
// type warns), and for the members of a value of a marked type; a field, a record field and a
// `class var` marked `deprecated` whose type is a deprecated type are inside the same mark, as a
// variable is. `inherited Step`, `inherited` alone and an overload picked among others are uses. `{$WARN deprecated OFF}` turns W0011 off from that point.

type
  TOld = class
    Size: Integer;
    procedure Run; virtual;
    function Clone: TOld;
  end deprecated;

  TBase = class
    procedure Step; virtual; deprecated 'use Walk';
  end;
  TChild = class(TBase)
    procedure Step; override;
  end;

  THolder = class
    Kept: TOld deprecated;
    class var Shared: TOld deprecated;
  end;
  TRec = record
    Kept: TOld deprecated;
  end;

function TOld.Clone: TOld;
begin
  Result := TOld.Create;
  Result.Size := Size;
end;

procedure TOld.Run;
begin
end;

procedure TBase.Step;
begin
end;

procedure TChild.Step;
begin
  inherited Step;
  inherited;
end;

procedure Old(N: Integer); deprecated;
begin
  if N > 0 then
    Old(N - 1);
end;

procedure Older; deprecated;
begin
  Old(1);
end;

procedure Trial; experimental;
begin
  Old(2);
end;

procedure Pick(X: Integer); overload; deprecated;
begin
end;

procedure Pick(S: string); overload;
begin
end;

procedure Use(O: TOld);
begin
  O.Run;
  WriteLn(O.Size);
end;

var
  B: TBase;
begin
  B := TChild.Create;
  B.Step;
  B.Free;
  Pick('a');
  Pick(1);
  Older;
  Trial;
{$WARN deprecated OFF}
  Old(3);
  Trial;
end.
