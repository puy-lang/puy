program ClassErrors;

// Diagnostics of class declarations and their fields (objects.md chapters 1-4, 16): E0701 (a
// field with the name of a field the class's declaration sees in an ancestor; a hidden one may be
// declared again), E0743 (a strict private field outside the class), E0702 with its note (an
// ancestor to a descendant), E0718 with its note, E0723 (a forward declaration without a full
// one), E0401 (a modifier written twice), E0700 (no such field; a member Puy does not have),
// E0733 (a class that is not an exception class in `raise` / `on`), E0705 (`Create` with
// arguments), E0748 (`[weak]` on a class managed by hand), E3200 for what later items implement
// and for the reserved names of later items (with their milestone), and E0761 for the generic
// `TFunc` without type arguments (generics.md 2.3).

type
  TFwd = class;
  TBase = class
    Name: string;
  strict private
    Secret: Integer;
  protected
    Shared: Integer;
  end;
  TChild = class(TBase)
    Name: Integer;
    Secret: string;
    Extra: Boolean;
  end;
  TTwice = class sealed sealed
  end;
  TLater = class abstract
    Count: Integer;
    class var Total: Integer;
    const Max = 3;
    type TInner = Integer;
    procedure Run;
    property P: Integer read Count;
    [weak] Link: TLater;
  end;
  TWithIntf = class(TObject, IInterface)
  end;
  TRef = class of TBase;
  TOld = class(TObject) platform deprecated 'use TBase';

var
  B: TBase;
  C: TChild;
  O: TObject;
  I: Integer;
  S: string;
  K: TClass;
  F: TFunc;
  G: TGUID;

begin
  B := TBase.Create;
  C := B;
  C := O;
  B := C;
  WriteLn(B);
  I := B.Secret;
  I := B.Shared;
  I := B.Missing;
  I := B.InstanceSize;
  S := B.ClassName;
  B := TBase.Create(1);
  B.Name;
  try
    I := 2;
  except
    on E: TBase do
      I := 3;
  end;
  B.Free(1);
  raise B;
end.
