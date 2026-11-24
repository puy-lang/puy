program DumpValueEquality;

// The generated functions of the key equality and hash (equality.md 8.3, 9): `<eq:…>` /
// `<hash:…>` of a record, of a record nested in it, of a dynamic array and of a static array,
// called from the `Equals` / `GetHashCode` generated for a `[ValueEquality]` class; the `hir`
// dump shows them under `.generated`, and the calls as `call KeyEquals<…>`.

type
  TInner = record
    S: string;
    F: Single;
  end;

  TPoint = record
    X: Integer;
    Inner: TInner;
  end;

  TDigits = array[0..2] of Byte;

  [ValueEquality]
  TShape = class
  public
    Origin: TPoint;
    Path: array of TPoint;
    Digits: TDigits;
    Owner: TObject;
  end;

var
  S: TShape;
begin
  // Never true: names the class, so that its table keeps the generated methods (units.md 8.3)
  if ParamCount > 100 then
    S := TShape.Create;
end.
