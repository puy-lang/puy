program GenericNestedTypeRefErrors;

// A name written after an instantiation in a type position must name a type nested in it
// (generics.md 2.5), as through an alias of the instantiation: no such name (a field is not one
// either) is E0700 and a constant E0708, a nested type with type arguments it does not take or a
// generic one without them E0761, a nested helper E0753 (its declaration in a generic type is
// E0757, generics.md 2.1), a name a nested record lacks E0700 (as a class), and `TArray<…>` has
// no nested types to name (E0708). A helper's parent written so is resolved too (E0753 for a type
// that is not a helper).

type
  TOuter<T> = class
  public
    type
      TInner<U> = class
      end;
      TCell = record
        Value: T;
      end;
      THelp = class helper for TObject
      end;
    const
      Limit = 3;
  public
    Field: T;
  end;

  TPlainHelper = class helper(TOuter<Integer>.TCell) for TObject
  end;

var
  A: TOuter<Integer>.TMissing;
  B: TOuter<Integer>.Field;
  C: TOuter<Integer>.Limit;
  D: TOuter<Integer>.TCell<string>;
  E: TOuter<Integer>.TInner;
  F: TOuter<Integer>.TInner<string, string>;
  G: TOuter<Integer>.TMissing<string>;
  H: TOuter<Integer>.THelp;
  I: TArray<Integer>.TItem;
  J: TOuter<Integer>.TCell.TDeep;
begin
end.
