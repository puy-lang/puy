program GenericNestedHeadingErrors;

// The body's heading of a method of a type nested in a type (generics.md 1.2): type parameters
// of the outer type or of the nested type not written as declared, or those of the method left
// out, are E0757 with a fix, and the method's declaration then counts as having a body (no E0723
// on top of it). An overloaded name is not marked, since the heading does not tell which
// declaration it meant: E0723 on the declaration rather than E0701 on a later correct body.

type
  TG<T> = class
  type
    TCell = class
      Value: Integer;
      procedure Reset;
      function Pick<U>(const A: U): U;
      procedure Clear; overload;
      procedure Clear(N: Integer); overload;
    end;
  end;

  THolder = class
  type
    TBox<T> = class
      procedure Show;
    end;
  end;

procedure TG<X>.TCell.Reset;
begin
  Value := 0;
end;

function TG<T>.TCell.Pick(const A: U): U;
begin
  Result := A;
end;

procedure TG<X>.TCell.Clear;
begin
  Value := 0;
end;

procedure TG<T>.TCell.Clear(N: Integer);
begin
  Value := N;
end;

procedure THolder.TBox<X>.Show;
begin
end;

begin
end.
