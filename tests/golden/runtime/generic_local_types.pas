program GenericLocalTypes;

// Types declared in a generic body (generics.md 2.6): a local record using the routine's type
// parameter, compared with `KeyEquals` / `KeyHash`, found by `TList<TLoc>.IndexOf` and used as the
// key of `TDictionary<TLoc, T>`; a method of the local record; a local `[ValueEquality]` class;
// local types of a method of a generic type, of a generic method and of a nested routine. Each
// instantiation of the routine has its own types (their `class var`s count apart, and their
// classes differ).

uses
  System.SysUtils, System.Generics.Collections;

function Check<T>(const X: T): Boolean;
type
  TLoc = record
    V: T;
  end;
var
  L: TLoc;
begin
  L := Default(TLoc);
  L.V := X;
  Result := KeyEquals(L, L);
end;

function Use<T>(const X, Y: T): string;
type
  TLoc = record
    V: T;
    N: Integer;
    function Show: string;
  end;
  TPair = record
    A, B: TLoc;
  end;
  [ValueEquality]
  TCell = class
    V: T;
    class var Made: Integer;
  end;
function TLoc.Show: string;
begin
  Result := 'loc' + IntToStr(N);
end;
var
  L: TLoc = Default(TLoc);
  M: TLoc = Default(TLoc);
  P: TPair = Default(TPair);
  List: TList<TLoc>;
  Dict: TDictionary<TLoc, T>;
  C1, C2: TCell;
begin
  L.V := X;
  L.N := 1;
  M.V := Y;
  M.N := 2;
  P.A := L;
  P.B := M;
  Result := L.Show + ' ' + M.Show
    + ' eq=' + BoolToStr(KeyEquals(L, M), True)
    + ' pair=' + BoolToStr(KeyEquals(P, P), True)
    + ' hash=' + BoolToStr(KeyHash(L) = KeyHash(L), True);
  List := TList<TLoc>.Create;
  try
    List.Add(L);
    List.Add(M);
    Result := Result + ' index=' + IntToStr(List.IndexOf(M));
  finally
    List.Free;
  end;
  Dict := TDictionary<TLoc, T>.Create;
  try
    Dict.Add(L, X);
    Dict.AddOrSetValue(M, Y);
    Dict.AddOrSetValue(L, Y);
    Result := Result + ' count=' + IntToStr(Dict.Count)
      + ' key=' + BoolToStr(Dict.ContainsKey(M), True);
  finally
    Dict.Free;
  end;
  C1 := TCell.Create;
  C2 := TCell.Create;
  try
    C1.V := X;
    C2.V := X;
    Inc(TCell.Made);
    Result := Result + ' cell=' + BoolToStr(C1.Equals(C2), True)
      + ' made=' + IntToStr(TCell.Made) + ' ' + C1.ClassName;
  finally
    C1.Free;
    C2.Free;
  end;
end;

function CellClass<T>: TClass;
type
  TCell = class
    V: T;
  end;
begin
  Result := TCell;
end;

type
  TBox<T> = class
    function Pair<U>(const X: T; const Y: U): Boolean;
    function Same(const X: T): Boolean;
  end;

function TBox<T>.Pair<U>(const X: T; const Y: U): Boolean;
type
  TLoc = record
    A: T;
    B: U;
  end;
var
  L: TLoc = Default(TLoc);
begin
  L.A := X;
  L.B := Y;
  Result := KeyEquals(L, L);
end;

function TBox<T>.Same(const X: T): Boolean;

  function Inner: Boolean;
  type
    TIn = record
      V: T;
    end;
  var
    I: TIn = Default(TIn);
  begin
    I.V := X;
    Result := KeyHash(I) = KeyHash(I);
  end;

begin
  Result := Inner;
end;

var
  B: TBox<Integer>;
begin
  WriteLn(Check(1));
  WriteLn(Check('a'));
  WriteLn(Use<Integer>(1, 2));
  WriteLn(Use<Integer>(3, 3));
  WriteLn(Use<string>('a', 'b'));
  WriteLn(CellClass<Integer> = CellClass<Integer>);
  WriteLn(CellClass<Integer> = CellClass<string>);
  B := TBox<Integer>.Create;
  try
    WriteLn(B.Pair<string>(1, 'x'));
    WriteLn(B.Pair<Double>(1, 2.5));
    WriteLn(B.Same(4));
  finally
    B.Free;
  end;
end.
