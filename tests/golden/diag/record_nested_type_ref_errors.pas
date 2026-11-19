program RecordNestedTypeRefErrors;

// A name written after a record's name must name a type nested in it, as after a class's name
// (objects.md 2.2, generics.md 2.5): no such name (a field is not one either) is E0700, a
// constant E0708, and a nested type with type arguments it does not take E0761, also after an
// instantiation of a generic record.

type
  TRec = record
  type
    TInner = record
      X: Integer;
    end;
  const
    Max = 3;
  var
    A: Integer;
  end;

  TBox<T> = record
  type
    TCell = record
      Value: T;
    end;
  end;

var
  A: TRec.TMissing;
  B: TRec.Max;
  C: TRec.A;
  D: TRec.TInner.X;
  E: TBox<Integer>.TNone;
  F: TBox<Integer>.TCell<string>;
begin
end.
