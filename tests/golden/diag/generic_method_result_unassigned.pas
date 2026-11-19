program GenericMethodResultUnassigned;

// E2001 names a method of a generic type with its type parameters, as the other diagnostics do
// (`TVec<T>.Make`, not `TVec.Make`).

type
  TVec<T> = record
    class function Make(B: Boolean): TVec<T>; static;
  end;

  TBox<K, V> = class
    function Get(B: Boolean): V;
  end;

class function TVec<T>.Make(B: Boolean): TVec<T>;
begin
  if B then
    Result := Default(TVec<T>);
end;

function TBox<K, V>.Get(B: Boolean): V;
begin
  if B then
    Result := Default(V);
end;

begin
end.
