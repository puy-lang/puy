program EnumeratorErrors;

// The E0730 of `for..in` with `GetEnumerator` (objects.md 13.1): a class without
// `GetEnumerator` (the note on what `for..in` iterates over), a `GetEnumerator` that is a
// procedure, returns an `Integer`, takes a parameter or is a class method (each with its
// declaration), an enumerator whose `MoveNext` does not return `Boolean` and one whose `Current`
// cannot be read. The element is converted to the loop variable (E0702 at the collection), a
// `for var` variable cannot be assigned (E0707), and a `GetEnumerator` the code cannot see is
// E0743.

type
  TEnum = class
    function MoveNext: Boolean;
    function GetCurrent: string;
    property Current: string read GetCurrent;
  end;
  TNoMove = class
    function MoveNext: Integer;
    function Current: Integer;
  end;
  TNoCurrent = record
    function MoveNext: Boolean;
    procedure SetCurrent(V: Integer);
    property Current: Integer write SetCurrent;
  end;
  TNone = class
  end;
  TProcColl = class
    procedure GetEnumerator;
  end;
  TIntColl = class
    function GetEnumerator: Integer;
  end;
  TParamColl = class
    function GetEnumerator(N: Integer): TEnum;
  end;
  TClassColl = class
    class function GetEnumerator: TEnum;
  end;
  TMoveColl = class
    function GetEnumerator: TNoMove;
  end;
  TCurColl = class
    function GetEnumerator: TNoCurrent;
  end;
  TGood = class
    function GetEnumerator: TEnum;
  end;
  THidden = class
  strict private
    function GetEnumerator: TEnum;
  end;

function TEnum.MoveNext: Boolean; begin Result := False; end;
function TEnum.GetCurrent: string; begin Result := ''; end;
function TNoMove.MoveNext: Integer; begin Result := 0; end;
function TNoMove.Current: Integer; begin Result := 0; end;
function TNoCurrent.MoveNext: Boolean; begin Result := False; end;
procedure TNoCurrent.SetCurrent(V: Integer); begin end;
procedure TProcColl.GetEnumerator; begin end;
function TIntColl.GetEnumerator: Integer; begin Result := 0; end;
function TParamColl.GetEnumerator(N: Integer): TEnum; begin Result := nil; end;
class function TClassColl.GetEnumerator: TEnum; begin Result := nil; end;
function TMoveColl.GetEnumerator: TNoMove; begin Result := nil; end;
function TCurColl.GetEnumerator: TNoCurrent; begin Result := Default(TNoCurrent); end;
function TGood.GetEnumerator: TEnum; begin Result := nil; end;
function THidden.GetEnumerator: TEnum; begin Result := nil; end;

procedure Run(N: TNone; P: TProcColl; I: TIntColl; A: TParamColl; K: TClassColl;
  M: TMoveColl; C: TCurColl; G: TGood; H: THidden);
var
  X: Integer;
begin
  for var Y in N do
  begin
  end;
  for var Y in P do
  begin
  end;
  for var Y in I do
  begin
  end;
  for var Y in A do
  begin
  end;
  for var Y in K do
  begin
  end;
  for var Y in M do
  begin
  end;
  for var Y in C do
  begin
  end;
  for X in G do
  begin
  end;
  for var S in G do
    S := 'x';
  for var S in H do
  begin
  end;
end;

begin
end.
