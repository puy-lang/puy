program TypeDeclErrors;

// Errors in type declarations (types.md chapter 1, 2.1, 2.2).

var
  N: Integer;

procedure Main;
begin
end;

type
  TPoint = record
    X, Y: Integer;
    X: string;                  // E0701: duplicate field
  end;
  TPoint = record end;          // E0701: duplicate type name
  TSelf = record
    Next: TSelf;                // E0727: contains itself
  end;
  TA = record
    B: array[0..1] of TB;       // E0727: TA -> TB -> TA
  end;
  TB = record
    A: TA;
  end;
  TBack = array[3..1] of Integer;          // E0728: invalid bounds
  THuge = array[Integer] of Byte;          // E0728: too large
  TVariant = record
    Kind: Integer;
    case Boolean of                        // E3005
      True: (I: Integer);
  end;
  TPacked = packed record                  // W0016
    B: Byte;
  end;
  TDyn = array of TDyn;                    // E0727: an array of itself
  TMethods = record
    procedure Run;                         // E3200 (M3)
  end;
  TDistinct = type Integer;                // E3200 (M4)
  TUnknown = record
    F: TNowhere;                           // E0700
  end;
  TNotType = array[0..1] of Main;          // E0708
  TBadBound = array[0..N] of Integer;      // E0713: N is a variable

begin
end.
