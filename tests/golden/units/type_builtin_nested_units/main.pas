program Main;

// The built-ins that take a type accept a type of another unit qualified with the unit's name
// (`Shapes.TRec`), and a type nested in it (`Shapes.TC.TR`; units.md 3.2, generics.md 2.5), the
// same as through an alias.

uses Shapes;

type
  TAR = Shapes.TC.TR;
  TAE = Shapes.TC.TE;
  TARec = Shapes.TRec;

begin
  WriteLn(Default(Shapes.TC.TR).X, '[', Default(Shapes.TC.TR).S, '] ', Default(TAR).X);
  WriteLn(Ord(High(Shapes.TC.TE)), ' ', Ord(High(TAE)), ' ', SizeOf(Shapes.TC.TE));
  WriteLn(Default(Shapes.TRec).A, ' ', Default(TARec).A);
  // A value qualified with the unit's name is still a value
  WriteLn(Low(Shapes.Arr), ' ', High(Shapes.Arr));
end.
