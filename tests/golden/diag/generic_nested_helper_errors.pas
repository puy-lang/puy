// A helper declared in a generic class or record, or in a type nested in one, is E0757
// (generics.md 2.1): a helper is not copied for each instantiation, so its target would stay the
// open type. E0757 is the only error: the bodies of the helper's methods, a method without a
// body, and uses of the helper add no other.
program GenericNestedHelperErrors;

type
  TBase = class
  end;

  TOuter<T> = class
  type
    TCH = class helper for TBase
      function Name: string;
    end;
    TInner = class
    type
      TDeep = class helper for TBase
        procedure NoBody;
      end;
    end;
    TGen<U> = class
    type
      TGH = record helper for Integer
      end;
    end;
  end;

  TRec<T> = record
  type
    TRH = record helper for Integer
      function Twice: Integer;
    end;
    TCell = record
    type
      TCellH = record helper for string
      end;
    end;
  end;

  THolder = class
  type
    TBox<T> = class
    type
      TBH = class helper for TBase
      end;
    end;
  end;

function TOuter<T>.TCH.Name: string;
begin
  Result := 'base';
end;

function TRec<T>.TRH.Twice: Integer;
begin
  // Not checked: the helper is reported with E0757 alone
  Result := Missing * 2;
end;

var
  B: TBase;
begin
  B := TBase.Create;
  WriteLn(TOuter<Integer>.TCH.Name(B));
  B.Free;
end.
