program DefaultPropertyOverloads;

// Default array properties chosen by the types of the indices (objects.md 12.1, 12.2), which FPC
// 3.2.2 cannot declare: integer and string indices of one class, the same choice through the
// name (`X.Items[…]`), the number of indices choosing too (a `Byte` index converted to `Integer`
// or `Int64`), a child's default properties hiding the parent's,
// an interface's default property, and a record's written through a variable (a copy keeps its
// own value). The receiver, the indices from the left and the value are evaluated before the
// call (FPC evaluates them from the right).

type
  TBag = class(TRefObject)
  private
    FItems: array[0..3] of string;
    function GetItem(I: Integer): string;
    procedure SetItem(I: Integer; const Value: string);
    function GetByName(const Name: string): Integer;
    function GetWide(I: Int64; Tag: Boolean): string;
  public
    property Items[I: Integer]: string read GetItem write SetItem; default;
    property Items[const Name: string]: Integer read GetByName; default;
    property Items[I: Int64; Tag: Boolean]: string read GetWide; default;
  end;

  TNamedBag = class(TBag)
    function GetChar(C: Char): string;
    property Chars[C: Char]: string read GetChar; default;
  end;

  IList = interface
    function GetEntry(I: Integer): string;
    procedure SetEntry(I: Integer; const Value: string);
    property Entries[I: Integer]: string read GetEntry write SetEntry; default;
  end;

  TList = class(TRefObject, IList)
  private
    FEntries: array[0..1] of string;
  public
    function GetEntry(I: Integer): string;
    procedure SetEntry(I: Integer; const Value: string);
  end;

  TVec = record
    A: array[0..2] of Integer;
    function Get(I: Integer): Integer;
    procedure Put(I: Integer; V: Integer);
    property Items[I: Integer]: Integer read Get write Put; default;
  end;

function TBag.GetItem(I: Integer): string;
begin
  Result := FItems[I];
end;

procedure TBag.SetItem(I: Integer; const Value: string);
begin
  WriteLn('set ', I, ' ', Value);
  FItems[I] := Value;
end;

function TBag.GetByName(const Name: string): Integer;
var
  I: Integer;
begin
  Result := -1;
  for I := 0 to 3 do
    if FItems[I] = Name then
      Result := I;
end;

function TBag.GetWide(I: Int64; Tag: Boolean): string;
begin
  Result := 'wide';
end;

function TNamedBag.GetChar(C: Char): string;
begin
  Result := 'char ' + C;
end;

function TList.GetEntry(I: Integer): string;
begin
  Result := FEntries[I];
end;

procedure TList.SetEntry(I: Integer; const Value: string);
begin
  FEntries[I] := Value;
end;

function TVec.Get(I: Integer): Integer;
begin
  Result := A[I];
end;

procedure TVec.Put(I: Integer; V: Integer);
begin
  A[I] := V;
end;

function Index(N: Integer): Integer;
begin
  WriteLn('index ', N);
  Result := N;
end;

function Text(const S: string): string;
begin
  WriteLn('value ', S);
  Result := S;
end;

function Bag(B: TBag): TBag;
begin
  WriteLn('receiver');
  Result := B;
end;

var
  B: TBag;
  N: TNamedBag;
  L: IList;
  V, W: TVec;
  Small: Byte;
begin
  B := TBag.Create;
  B[0] := 'a';
  B[2] := 'c';
  WriteLn(B[0], B[2], ' ', B['c'], ' ', B.Items['zz'], ' ', B.Items[2]);
  Small := 2;
  WriteLn(B[Small], ' ', B[Small, True]);
  Bag(B)[Index(1)] := Text('b');
  WriteLn(B[1]);
  N := TNamedBag.Create;
  WriteLn(N['x'], ' ', TBag(N)['x']);
  L := TList.Create;
  L[1] := 'entry';
  WriteLn(L[1]);
  V[2] := 9;
  W := V;
  W[2] := 1;
  WriteLn(V[2], ' ', W[2]);
end.
