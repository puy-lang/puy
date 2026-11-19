program DumpAttrRecordNested;

// An attribute class nested in a record is named through the record's name, as one nested in a
// class is (objects.md 6, attributes.md 3): `[TRec.TMark]` finds `TRec.TMarkAttribute` and
// chooses its constructor (the `.attr` line of the `hir` dump).

type
  TRec = record
  type
    TMarkAttribute = class(TCustomAttribute)
    public
      constructor Create(N: Integer);
    end;
  end;

  TOuter = class
  type
    TTagAttribute = class(TCustomAttribute)
    end;
  end;

constructor TRec.TMarkAttribute.Create(N: Integer);
begin
end;

type
  [TRec.TMark(1)]
  TA = class
  end;

  [TRec.TMarkAttribute(2)]
  [TOuter.TTag]
  TB = class
  end;

begin
  WriteLn('ok');
end.
