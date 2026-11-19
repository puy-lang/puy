program DumpGenericLocalTypes;

// The names of the types declared in a generic body (generics.md 2.6, 7, 8): an instantiation is
// named after the routine's (`Find<Integer>.TLoc`, `TBox<Integer>.Get.TLoc`), its generated
// `<eq:…>` / `<hash:…>` take its full name, and a shared method of a local type is named in the
// generic form (`Find<T>.TLoc.Size`).

type
  TBox<T> = class
    function Get(const X: T): Boolean;
  end;

function Find<T>(const X: T): Integer;
type
  TLoc = record
    V: T;
    N: Integer;
    function Size: Integer;
  end;
function TLoc.Size: Integer;
begin
  Result := N + 1;
end;
var
  L: TLoc = Default(TLoc);
begin
  L.V := X;
  if KeyEquals(L, L) then
    Result := L.Size
  else
    Result := 0;
end;

function TBox<T>.Get(const X: T): Boolean;
type
  TLoc = record
    V: T;
  end;
var
  L: TLoc = Default(TLoc);
begin
  L.V := X;
  Result := KeyHash(L) = KeyHash(L);
end;

var
  B: TBox<Integer>;
begin
  WriteLn(Find(1) + Find(2.5));
  B := TBox<Integer>.Create;
  WriteLn(B.Get(3));
  B.Free;
end.
