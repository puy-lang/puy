program HelperErrors;

// E0753 (objects.md 10.2, 10.3, 10.6, 10.9): a type that cannot have a helper, or a helper of
// the other kind (with the fix swapping `class` and `record`); what a helper cannot declare
// (fields, constructors, virtual methods, a name the type has even if not visible, a member of
// `TObject`, a `GetEnumerator` that `for..in` never calls, a default array property); a parent that is not a helper or is
// for another type; a helper used as a type; `inherited` in a helper's method. Also E0701 for a
// name the parent helper has (10.6) and E0743 for the members of the type a helper cannot use
// (10.8). The methods with an error do not also get E0723 for their missing bodies. Writing
// `S.Length` is E0707, and a name no helper of a built-in type has is the E3200 of M5.

type
  IFoo = interface
  end;

  TFoo = class
  private
    FSecret: Integer;
  protected
    procedure Hidden;
  public
    Count: Integer;
    procedure Run;
  end;

  TPoint = record
    X: Integer;
  end;

  TProcType = procedure;

  TIntfHelper = record helper for IFoo           // E0753: an interface has no helper
  end;

  TWrongKind = record helper for TFoo            // E0753: a class has a class helper (fix)
  end;

  TWrongKind2 = class helper for Integer         // E0753: other types have a record helper (fix)
  end;

  TProcHelper = record helper for TProcType      // E0753: no helper for a procedural type
  end;

  TFooHelper = class helper for TFoo
  private
    Extra: Integer;                              // E0753: no fields
  public
    constructor Make;                            // E0753: no constructor
    procedure Virt; virtual;                     // E0753: no virtual methods
    procedure Run;                               // E0753: the class has `Run`
    procedure Hidden;                            // E0753: even a protected one
    function ClassName: string;                  // E0753: a member of `TObject`
    procedure Go;
  end;

  TPointHelper = record helper for TPoint
    procedure Show;
  end;

  TOther = record helper(TFooHelper) for TPoint  // E0753: the parent is for `TFoo`
  end;

  TNotParent = record helper(TPoint) for TPoint  // E0753: not a helper
  end;

  TChildHelper = record helper(TPointHelper) for TPoint
    procedure Show;                              // E0701: declared in the parent
  end;

  TIntsHelper = record helper for TArray<Integer>
    function GetEnumerator: Integer;             // E0753: never used by `for..in`
    function Get(I: Integer): Integer;
    property Items[I: Integer]: Integer read Get; default; // E0753: no default property
  end;

var
  H: TFooHelper;                                 // E0753: not a type
  S: string;
  N: Integer;
  Cur: Currency;

procedure TFoo.Hidden;
begin
end;

procedure TFoo.Run;
begin
end;

procedure TFooHelper.Go;
begin
  WriteLn(FSecret);                              // E0743: private to `TFoo`
  Hidden;                                        // E0743: protected in `TFoo`
  inherited Go;                                  // E0753: no `inherited`
end;

procedure TPointHelper.Show;
begin
  WriteLn(X);
end;

procedure TChildHelper.Show;
begin
end;

function TIntsHelper.GetEnumerator: Integer;
begin
  Result := 0;
end;

function TIntsHelper.Get(I: Integer): Integer;
begin
  Result := I;
end;

begin
  S := 'abc';
  N := 0;
  S.Length := 3;                                 // E0707: a read-only property
  N.Foo := 1;                                    // E0700: no helper has `Foo`
  WriteLn(Cur.ToHexString);                      // E3200: `Currency` has no members yet
end.
