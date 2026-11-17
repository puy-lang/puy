program PropertyErrors;

// Errors about properties (objects.md 2.3, 2.6): accessors that do not fit (E0746, with the
// expected heading in the notes), instance members as the accessors of a class property, the
// ignored specifiers (W0022), the COM specifiers (E3010), `implements` and `property Name;`
// (E3200; `Def` is a default array property), a name declared twice (E0701), reading a write-only property and writing a
// read-only one (E0707), a property passed to `var` / `out` parameters and `Inc` (E0706), writing a
// field or an element of a property's value (E0707), the wrong number of indices (E0705), a slice
// of an indexed property read or written (E0720), an instance property without an instance (E0700),
// and a property as a statement or called with arguments.

type
  TPoint = record
    X, Y: Integer;
  end;

  TFoo = class
  private
    FA: Integer;
    FPos: TPoint;
    FS: string;
    class var FCount: Integer;
    function GetA: Integer;
    procedure SetA(V: Integer);
    function GetB(I: Integer): Integer;
    class function GetCount: Integer;
  public
    property RO: Integer read FA;
    property WO: Integer write SetA;
    property Pos: TPoint read FPos write FPos;
    property S: string read FS write FS;
    property Items[I: Integer]: Integer read GetB;
    property Missing: Integer read Nope;
    property WrongType: string read GetA;
    property WrongWrite: Integer write GetA;
    property WrongIndexed[I: Integer]: Integer read GetA write SetA;
    property WrongIndex: Integer index 1 read GetA;
    property Dotted: Integer read FPos.X;
    property Stored: Integer read FA stored True default 0;
    property NoDefault: Integer read FA nodefault;
    property ReadOnly: Integer readonly;
    property WriteOnly: Integer writeonly;
    property DispId: Integer read FA dispid 3;
    property Impl: Integer read FA implements TObject;
    property RO: Integer read FA;
    property FA: Integer read FA;
    class property ByField: Integer read FA;
    class property ByMethod: Integer read GetA;
    class property ByClassMethod: Integer read GetCount;
    class property CountWrong: string read FCount;
    property Def[I: Integer]: Integer read GetB; default;
    property Name;
    class procedure Report;
  end;

function TFoo.GetA: Integer;
begin
  Result := 1;
end;

procedure TFoo.SetA(V: Integer);
begin
end;

function TFoo.GetB(I: Integer): Integer;
begin
  Result := I;
end;

class function TFoo.GetCount: Integer;
begin
  Result := FCount;
end;

class procedure TFoo.Report;
begin
  WriteLn(RO);
end;

procedure Bump(var X: Integer);
begin
  Inc(X);
end;

procedure Put(out X: Integer);
begin
  X := 1;
end;

var
  O: TFoo;
  N: Integer;
begin
  O := TFoo.Create;
  O.RO := 1;
  N := O.WO;
  Bump(O.RO);
  Put(O.RO);
  Inc(O.RO);
  Bump(O.Items[1]);
  O.Pos.X := 1;
  O.S[1] := 'a';
  N := O.Items;
  N := O.Items[1, 2];
  O.Items[1] := 2;
  N := O.Items[1..2];
  O.Items[1..2] := 1;
  O.Items[1, 2..3] := 1;
  N := TFoo.RO;
  O.RO;
  N := O.RO(1);
  O.Free;
end.
