program GenericNestedTypeExprErrors;

// A name written after an instantiation in an expression gets the diagnostics of a type
// position (generics.md 1.4, 2.5): no such member is E0700, a nested type that is not generic
// with type arguments or a generic one with the wrong number of them E0761, and a nested generic
// type without type arguments E0761 (also through an alias of the instantiation, through a
// class that is not generic and through a generic record).

type
  TOuter<T> = class
  public
    type
      TInner<U> = class
      end;
      TCell = class
      end;
  end;

  THolder = class
  public
    type
      TBox<T> = class
      end;
  end;

  TRec<T> = record
  public
    type
      TTag<U> = class
      end;
  end;

  TIntOuter = TOuter<Integer>;

var
  O: TObject;
begin
  O := TOuter<Integer>.TMissing<string>.Create;
  O := TOuter<Integer>.TMissing.Create;
  O := TOuter<Integer>.TCell<string>.Create;
  O := TOuter<Integer>.TInner<string, string>.Create;
  O := TOuter<Integer>.TInner.Create;
  O := TIntOuter.TInner.Create;
  O := THolder.TBox.Create;
  O := TRec<Integer>.TTag.Create;
end.
