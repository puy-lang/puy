program GenericHeadingMissingParamsAmbiguous;

// A heading that leaves out the type parameters of a generic type overloaded by their number,
// where more than one of the types declares the method (generics.md 1.2): E0757 without a fix,
// showing the list of the first such type, and E0723 stays on the declarations without a body,
// since the heading does not tell which one it meant (as for an overloaded method).

type
  TPair<K> = class
    procedure One;
  end;
  TPair<K, V> = class
    procedure Both;
  end;
  TPair<A, B, C> = class
    procedure Both;
  end;

procedure TPair<K>.One;
begin
end;

procedure TPair.Both;
begin
end;

procedure TPair<K, V>.Both;
begin
end;

begin
end.
