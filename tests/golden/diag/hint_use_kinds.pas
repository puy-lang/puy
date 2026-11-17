program HintUseKinds;

// More kinds of use reported by W0011 / W0012 (attributes.md 6.2): a parent class and an
// interface written in a class's heading, an attribute class resolved from an attribute's name,
// and a write to a deprecated property.

type
  TOldBase = class(TRefObject)
  end deprecated;
  IOld = interface
  end experimental;
  TKid = class(TOldBase, IOld)
  end;

  OldAttribute = class(TCustomAttribute)
  end deprecated 'use New';
  [Old] TMarked = class(TRefObject)
  end;

  TBox = class(TRefObject)
  private
    FV: Integer;
  public
    property V: Integer read FV write FV deprecated;
  end;

var
  F: TBox;
begin
  F := TBox.Create;
  F.V := 1;
  WriteLn(TKid.Create <> nil, TMarked.Create <> nil);
end.
