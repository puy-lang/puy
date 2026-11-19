program GenericHeadingMissingParams;

// The body's heading of a method of a generic type that leaves out the type's parameters
// (generics.md 1.2): E0757 on the type's name with a fix that inserts the declared list, without
// E0700 for the name or E0723 on the method's declaration. A nested generic type left out is the
// same; for types overloaded by the number of type parameters, the one declaring the next name is
// picked. Only the first part left out of a heading is reported; `puy fix` writes the rest on its
// next pass, and the fixed program has no errors. A class of the same name that is not generic is
// the one a heading without type parameters names; a type of the same name that is not a class,
// record or helper cannot be, so the heading still means the generic type. A generic method of a
// nested type that is not generic, written without its type parameters, is E0757 as for
// `TUtil.Max` (generics.md 1.2).

type
  TG<T> = class
  type
    TCell = class
      Value: Integer;
      procedure Reset;
    end;
    TInner<U> = class
      procedure Run;
      procedure Stop;
      procedure Wait;
    end;
  public
    procedure Reset;
    function Map<U>(const A: U): U;
  end;

  TPair<K> = class
    procedure One;
  end;
  TPair<K, V> = class
    procedure Two;
  end;

  TBoth = class
    procedure Run;
  end;

  TNum = Integer;
  TNum<T> = class
    procedure Run;
  end;

  TOuter = class
  type
    TInner = class
      function Map<U>(const A: U): U;
    end;
  end;
  TBoth<T> = class
    procedure Run;
  end;

procedure TG<T>.Reset;
begin
end;

procedure TG<T>.TCell.Reset;
begin
  Value := 0;
end;

function TG<T>.Map<U>(const A: U): U;
begin
  Result := A;
end;

procedure TG<T>.TInner<U>.Run;
begin
end;

procedure TG<T>.TInner<U>.Stop;
begin
end;

procedure TG<T>.TInner<U>.Wait;
begin
end;

procedure TPair<K>.One;
begin
end;

procedure TPair<K, V>.Two;
begin
end;

procedure TBoth.Run;
begin
end;

procedure TBoth<T>.Run;
begin
end;

procedure TNum<T>.Run;
begin
end;

function TOuter.TInner.Map<U>(const A: U): U;
begin
  Result := A;
end;

begin
end.
