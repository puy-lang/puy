// A helper declared in a class or record (objects.md 10.4) works where the type declaring it is
// visible: in the methods of that type, later in the file, through more than one level of
// nesting, from a `private` section in the same file, and in a routine whose local `type` section
// declares the outer type; `TOuter.TH.Pick(B, …)` still works too.
program HelperNested;

type
  TBox = class
    V: Integer;
  end;

  TOuter = class
  type
    TH = class helper for TBox
      function Pick(N: Integer): Integer;
    end;
  private
  type
    TQuiet = record helper for Integer
      function Twice: Integer;
    end;
  public
    procedure Run(B: TBox);
  end;

  TRec = record
  type
    TRH = record helper for Integer
      function Neg: Integer;
    end;
  end;

  TMid = class
  type
    TInner = class
    type
      TDeep = class helper for TBox
        function Deep: string;
      end;
    end;
  end;

function TOuter.TH.Pick(N: Integer): Integer;
begin
  Result := V + N;
end;

function TOuter.TQuiet.Twice: Integer;
begin
  Result := Self * 2;
end;

procedure TOuter.Run(B: TBox);
begin
  WriteLn(B.Pick(1));
end;

function TRec.TRH.Neg: Integer;
begin
  Result := -Self;
end;

function TMid.TInner.TDeep.Deep: string;
begin
  Result := 'deep ' + V.ToString;
end;

procedure Local(B: TBox);
type
  TLocalOuter = class
  type
    TLH = class helper for TBox
      function Loc: string;
    end;
  end;

  function TLocalOuter.TLH.Loc: string;
  begin
    Result := 'local ' + V.ToString;
  end;

begin
  WriteLn(B.Loc);
end;

var
  B: TBox;
  O: TOuter;
  I: Integer;
begin
  B := TBox.Create;
  B.V := 10;
  O := TOuter.Create;
  O.Run(B);
  O.Free;
  WriteLn(B.Pick(2));
  WriteLn(TOuter.TH.Pick(B, 3));
  WriteLn(B.Deep);
  Local(B);
  I := 21;
  WriteLn(I.Twice);
  WriteLn(I.Neg);
  B.Free;
end.
