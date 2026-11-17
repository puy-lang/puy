program GenericsUnsupported;

// Generic declarations M4a has implemented (generics.md, the preamble): a generic record or class
// is declared (M4-8) and instantiated (M4-11), a generic routine or method is declared (M4-14),
// and the other generic types are declared and instantiated also where only a type name is taken
// (parents, `class of`, `on E:`; M4-16). An undeclared name with type arguments is E0700;
// `TArray<T>` and `TProc` stay as they are.

type
  TBox<T> = record
    Value: T;
  end;
  TPlain = class
    function Map<U>(X: Integer): Integer;
  end;
  TIntBox = TBox<Integer>;
  TIntList = class(TList<Integer>)
  end;
  TCounted = class(TRefObject, IEnumerable<Integer>)
  end;
  TListClass = class of TList<Integer>;
  IIntSource = interface(IEnumerable<Integer>)
  end;

var
  A: TArray<Integer>;
  L: TList<string>;
  P: TProc;

function TPlain.Map<U>(X: Integer): Integer;
begin
  Result := X;
end;

function Identity<T>(const X: T): T;
begin
  Result := X;
end;

procedure Take(const B: TBox<string>);
begin
end;

begin
  A := [1];
  P := nil;
  try
    WriteLn(Length(A), Assigned(P));
  except
    on E: EWrapped<Integer> do
      WriteLn('wrapped');
  end;
end.
