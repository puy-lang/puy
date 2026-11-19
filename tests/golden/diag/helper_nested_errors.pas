// A helper nested in a class does not work before its declaration (objects.md 10.4) nor outside
// the routine whose local `type` section declares its outer type, and a nested helper and a
// top-level helper with the same member in one step are ambiguous (E0754), naming the nested
// helper with its outer type (`TOuter.TH`).
program HelperNestedErrors;

type
  TBox = class
    V: Integer;
  end;

procedure Early(B: TBox);
begin
  WriteLn(B.Pick);
end;

procedure Local(B: TBox);
type
  TLocalOuter = class
  type
    TLH = class helper for TBox
      function Loc: Integer;
    end;
  end;

  function TLocalOuter.TLH.Loc: Integer;
  begin
    Result := V;
  end;

begin
  WriteLn(B.Loc);
end;

type
  TOuter = class
  type
    TH = class helper for TBox
      function Pick: Integer;
      function Same: Integer;
    end;
  end;

  TTop = class helper for TBox
    function Same: Integer;
  end;

function TOuter.TH.Pick: Integer;
begin
  Result := V;
end;

function TOuter.TH.Same: Integer;
begin
  Result := 1;
end;

function TTop.Same: Integer;
begin
  Result := 2;
end;

var
  B: TBox;
begin
  B := TBox.Create;
  WriteLn(B.Pick);
  WriteLn(B.Same);
  WriteLn(B.Loc);
  B.Free;
end.
