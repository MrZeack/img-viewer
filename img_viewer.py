#!/usr/bin/env python3

#!/usr/bin/env python3

import os
import sys
import shutil
import math
import signal
import threading
from datetime import datetime
from pathlib import Path
from collections import OrderedDict
import tkinter as tk
from tkinter import messagebox, ttk
from PIL import Image, ImageTk
from PIL.ExifTags import TAGS

# Pre-compute common values
PHI = 1.618
PHI_INV = 0.618
COMMON_SHUTTER_SPEEDS = [
    (1/4000, '1/4000'), (1/2000, '1/2000'), (1/1000, '1/1000'),
    (1/500, '1/500'), (1/250, '1/250'), (1/125, '1/125'),
    (1/60, '1/60'), (1/30, '1/30'), (1/15, '1/15'),
    (1/8, '1/8'), (1/4, '1/4'), (1/2, '1/2')
]
FLASH_MODES = {
    0: 'No Flash', 1: 'Flash Fired', 5: 'Flash Fired, No Return',
    7: 'Flash Fired, Return', 16: 'No Flash, Compulsory',
    24: 'No Flash, Auto', 25: 'Flash Fired, Auto',
    31: 'Flash Fired, Auto, Return', 32: 'No Flash Available'
}

class LRUCache:
    """High-performance LRU cache for image data"""
    def __init__(self, max_size=50):
        self.max_size = max_size
        self.cache = OrderedDict()
        
    def get(self, key):
        if key in self.cache:
            # Move to end (most recently used)
            self.cache.move_to_end(key)
            return self.cache[key]
        return None
    
    def put(self, key, value):
        if key in self.cache:
            # Update existing key
            self.cache.move_to_end(key)
        else:
            # Add new key, remove oldest if needed
            if len(self.cache) >= self.max_size:
                # Remove least recently used (first item)
                self.cache.popitem(last=False)
        
        self.cache[key] = value
    
    def clear(self):
        self.cache.clear()
    
    def __len__(self):
        return len(self.cache)

class ImageViewer:
    def __init__(self, root):
        self.root = root
        self.root.title("Image Visualizer")
        
        # Set initial window size for better layout with sidebar
        self.root.geometry("1400x900")
        self.root.configure(bg='black')
        
        # Set as main window and bring to front
        self.root.lift()
        self.root.attributes('-topmost', True)
        self.root.after_idle(lambda: self.root.attributes('-topmost', False))
        
        # Try to maximize window on different platforms
        try:
            self.root.state('zoomed')  # Windows/Linux
        except tk.TclError:
            try:
                self.root.attributes('-zoomed', True)  # Some Linux distributions
            except tk.TclError:
                # Fallback - just ensure decent size
                self.root.geometry("1400x900")
        
        # Get current directory and find JPG images
        self.current_dir = Path.cwd()
        self.image_files = self.get_jpg_files()
        self.current_index = 0
        
        if not self.image_files:
            messagebox.showerror("Error", "No se encontraron imágenes JPG en el directorio actual")
            sys.exit()
        
        # Image display variables - use __slots__ equivalent
        self.base_zoom = 1.0
        self.zoom_factor = 1.0
        self.image_x = self.image_y = 0
        self.drag_x = self.drag_y = 0
        
        # Optimized caches with LRU management
        self.image_cache = LRUCache(max_size=50)  # Increased for better performance
        self.metadata_cache = LRUCache(max_size=30)  # Cache metadata extraction
        self.last_canvas_size = (0, 0)
        
        # Composition lines - use tuple for immutable data
        self.show_composition = False
        self.composition_index = 0
        self.composition_types = (
            'rule_of_thirds', 'golden_ratio', 'diagonal', 'triangle',
            'golden_spiral', 'center_lines', 'phi_grid', 'dynamic_symmetry',
            'armature_of_rectangle', 'rabatment', 'harmonious_triangles',
            'sinister_diagonal', 'aspect_ratio_guides'
        )
        
        # Cache composition lines for performance
        self.composition_cache = {}
        self.last_composition_params = None
        
        # Image comparison bookmarks (1-9 keys) - optimized
        self.bookmarks = {}
        self.bookmark_message = ""  # Status bar message
        self.message_timeout = None  # Timer for clearing messages
        
        # Undo functionality - track last moved image
        self.last_moved = None  # (source_path, dest_path, dest_dir_type)
        
        # Background preloading
        self.preload_cache = {}  # Cache for preloaded images
        self.preload_lock = threading.Lock()
        self.stop_preloading = threading.Event()
        
        # Create best and worse directories if they don't exist
        self.best_dir = self.current_dir / "best"
        self.best_dir.mkdir(exist_ok=True)
        
        self.worse_dir = self.current_dir / "worse"
        self.worse_dir.mkdir(exist_ok=True)
        
        self.setup_ui()
        self.load_current_image()
        self.bind_events()
        self.setup_signal_handlers()
    
    def get_jpg_files(self):
        """Get all JPG files in current directory"""
        extensions = ['*.jpg', '*.jpeg', '*.JPG', '*.JPEG']
        jpg_files = []
        for ext in extensions:
            jpg_files.extend(list(self.current_dir.glob(ext)))
        return sorted(jpg_files)
    
    def setup_ui(self):
        """Setup the user interface"""
        # Create horizontal layout container
        self.content_frame = tk.Frame(self.root, bg='black')
        self.content_frame.pack(fill=tk.BOTH, expand=True)
        
        # Left frame for image display
        self.main_frame = tk.Frame(self.content_frame, bg='black')
        self.main_frame.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        
        # Canvas for image display
        self.canvas = tk.Canvas(self.main_frame, bg='black', highlightthickness=0)
        self.canvas.pack(fill=tk.BOTH, expand=True)
        
        # Setup metadata sidebar
        self.setup_metadata_sidebar()
        
        # Status bar (bottom of entire window)
        self.status_frame = tk.Frame(self.root, bg='gray20', height=30)
        self.status_frame.pack(fill=tk.X, side=tk.BOTTOM)
        self.status_frame.pack_propagate(False)
        
        self.status_label = tk.Label(
            self.status_frame, 
            text="", 
            bg='gray20', 
            fg='white', 
            font=('Arial', 10)
        )
        self.status_label.pack(side=tk.LEFT, padx=10, pady=5)
        
        # Instructions
        instructions = "← → : Navegar | Rueda: Zoom | L: Like | W: Worse | U: Undo | C: Composición | ↑↓: Cambiar | 1-9: Bookmarks | 0: Limpiar"
        self.instructions_label = tk.Label(
            self.status_frame, 
            text=instructions, 
            bg='gray20', 
            fg='lightgray', 
            font=('Arial', 9)
        )
        self.instructions_label.pack(side=tk.RIGHT, padx=10, pady=5)
    
    def setup_metadata_sidebar(self):
        """Setup metadata sidebar"""
        # Right sidebar frame
        self.sidebar_frame = tk.Frame(
            self.content_frame, 
            bg='gray25', 
            width=320
        )
        self.sidebar_frame.pack(side=tk.RIGHT, fill=tk.Y, padx=(2, 0))
        self.sidebar_frame.pack_propagate(False)  # Maintain fixed width
        
        # Sidebar title
        title_label = tk.Label(
            self.sidebar_frame,
            text="METADATOS",
            bg='gray25',
            fg='white',
            font=('Arial', 11, 'bold'),
            pady=8
        )
        title_label.pack(fill=tk.X)
        
        # Separator line
        separator = tk.Frame(self.sidebar_frame, bg='gray40', height=1)
        separator.pack(fill=tk.X, padx=10, pady=(0, 8))
        
        # Scrollable frame for metadata
        self.setup_scrollable_metadata()
    
    def setup_scrollable_metadata(self):
        """Setup scrollable metadata display"""
        # Create scrollable frame
        canvas = tk.Canvas(
            self.sidebar_frame, 
            bg='gray25', 
            highlightthickness=0,
            width=300
        )
        
        scrollbar = ttk.Scrollbar(
            self.sidebar_frame, 
            orient="vertical", 
            command=canvas.yview
        )
        
        self.scrollable_frame = tk.Frame(canvas, bg='gray25')
        
        # Configure scrolling
        self.scrollable_frame.bind(
            "<Configure>",
            lambda e: canvas.configure(scrollregion=canvas.bbox("all"))
        )
        
        canvas.create_window((0, 0), window=self.scrollable_frame, anchor="nw")
        canvas.configure(yscrollcommand=scrollbar.set)
        
        # Pack scrollable components
        canvas.pack(side="left", fill="both", expand=True, padx=(10, 0))
        scrollbar.pack(side="right", fill="y")
        
        # Store references
        self.metadata_canvas = canvas
        self.metadata_scrollbar = scrollbar
    
    def bind_events(self):
        """Bind keyboard and mouse events"""
        self.root.bind('<Key>', self.on_key_press)
        self.root.focus_set()
        
        # Mouse events for zoom and pan
        self.canvas.bind('<MouseWheel>', self.on_mouse_wheel)
        self.canvas.bind('<Button-4>', self.on_mouse_wheel)  # Linux scroll up
        self.canvas.bind('<Button-5>', self.on_mouse_wheel)  # Linux scroll down
        self.canvas.bind('<Button-1>', self.on_mouse_click)
        self.canvas.bind('<B1-Motion>', self.on_mouse_drag)
        self.canvas.bind('<ButtonRelease-1>', self.on_mouse_release)
        
        # Configure resize event
        self.canvas.bind('<Configure>', self.on_canvas_resize)
    
    def load_current_image(self):
        """Load and display the current image with lazy loading"""
        if not self.image_files:
            return
        
        try:
            image_path = self.image_files[self.current_index]
            self.image_path = image_path
            
            # Check if image is preloaded
            with self.preload_lock:
                preloaded = self.preload_cache.get(str(image_path))
            
            if preloaded:
                # Use preloaded image data
                self.original_image = preloaded['image']
                self.original_size = preloaded['size'] 
                self.original_mode = preloaded['mode']
                
                # Remove from preload cache as it's now active
                with self.preload_lock:
                    del self.preload_cache[str(image_path)]
            else:
                # Load image with lazy approach - only read metadata initially
                with Image.open(image_path) as img:
                    # Store image dimensions without loading full image data
                    self.original_size = img.size
                    self.original_mode = img.mode
                    
                    # Only load a low-resolution version for initial display
                    # This reduces memory usage and improves loading speed
                    max_preview_size = 1024
                    if max(img.size) > max_preview_size:
                        # Create thumbnail for initial display
                        ratio = max_preview_size / max(img.size)
                        preview_size = (int(img.size[0] * ratio), int(img.size[1] * ratio))
                        img.thumbnail(preview_size, Image.Resampling.BILINEAR)
                    
                    # Convert to RGB if necessary (for JPEG optimization)
                    if img.mode != 'RGB':
                        img = img.convert('RGB')
                    self.original_image = img.copy()
            
            # Reset zoom and position for new image
            self.zoom_factor = 1.0
            self.calculate_fit_to_window()
            self.image_x = 0
            self.image_y = 0
            
            # Clear cache when loading new image
            self.image_cache.clear()
            self.composition_cache.clear()
            self.last_composition_params = None
            
            self.update_display()
            self.update_status()
            self.update_metadata()
            
            # Start background preloading for adjacent images
            self.start_preloading()
            
        except Exception as e:
            messagebox.showerror("Error", f"Error al cargar la imagen: {str(e)}")
    
    def calculate_fit_to_window(self):
        """Calculate base zoom to fit image in window"""
        if not hasattr(self, 'original_size'):
            return
        
        canvas_width = self.canvas.winfo_width()
        canvas_height = self.canvas.winfo_height()
        
        if canvas_width <= 1 or canvas_height <= 1:
            self.base_zoom = 1.0
            return
        
        orig_width, orig_height = self.original_size
        
        # Calculate scale factors for width and height
        scale_x = canvas_width / orig_width
        scale_y = canvas_height / orig_height
        
        # Use the smaller scale to maintain aspect ratio
        self.base_zoom = min(scale_x, scale_y)
    
    def get_cached_image(self, width, height):
        """Get cached resized image or create new one with LRU management"""
        cache_key = (self.current_index, width, height)  # Include image index for better cache management
        
        # Try to get from cache
        cached_photo = self.image_cache.get(cache_key)
        if cached_photo:
            return cached_photo
        
        # Load full resolution image if needed for high zoom levels
        zoom_level = self.base_zoom * self.zoom_factor
        if zoom_level > 1.0 and hasattr(self, 'image_path'):
            # Need to load full resolution image
            with Image.open(self.image_path) as full_img:
                if full_img.mode != 'RGB':
                    full_img = full_img.convert('RGB')
                source_image = full_img.copy()
        else:
            # Use the preview image for lower zoom levels
            source_image = self.original_image
        
        # Create new resized image with adaptive resampling
        pixel_count = width * height
        if pixel_count > 2000000:  # Very large display - use fastest
            resized_image = source_image.resize((width, height), Image.Resampling.NEAREST)
        elif pixel_count > 1000000:  # Large display - balance speed/quality  
            resized_image = source_image.resize((width, height), Image.Resampling.BILINEAR)
        else:  # Small to medium display - highest quality
            resized_image = source_image.resize((width, height), Image.Resampling.LANCZOS)
        
        photo = ImageTk.PhotoImage(resized_image)
        self.image_cache.put(cache_key, photo)
        
        return photo
    
    def update_display(self):
        """Update the image display with current zoom and position"""
        if not hasattr(self, 'original_size'):
            return
        
        # Calculate total zoom (base zoom to fit window * user zoom)
        total_zoom = self.base_zoom * self.zoom_factor
        
        # Calculate new size based on original dimensions
        orig_width, orig_height = self.original_size
        new_width = int(orig_width * total_zoom)
        new_height = int(orig_height * total_zoom)
        
        # Use cached image if available
        self.photo = self.get_cached_image(new_width, new_height)
        
        # Clear canvas and display image
        self.canvas.delete("all")
        
        # Center image if smaller than canvas
        canvas_width = self.canvas.winfo_width()
        canvas_height = self.canvas.winfo_height()
        
        if new_width < canvas_width:
            self.image_x = (canvas_width - new_width) // 2
        if new_height < canvas_height:
            self.image_y = (canvas_height - new_height) // 2
        
        self.canvas.create_image(self.image_x, self.image_y, anchor=tk.NW, image=self.photo)
        
        # Draw composition lines if enabled
        if self.show_composition:
            self.draw_composition_lines()
    
    def start_preloading(self):
        """Start background preloading of adjacent images"""
        if len(self.image_files) <= 1:
            return
            
        # Stop any existing preloading
        self.stop_preloading.set()
        
        # Start new preloading thread
        self.stop_preloading.clear()
        preload_thread = threading.Thread(target=self.preload_adjacent_images, daemon=True)
        preload_thread.start()
    
    def preload_adjacent_images(self):
        """Preload next and previous images in background"""
        if not self.image_files:
            return
            
        # Get indices of adjacent images
        current_idx = self.current_index
        next_idx = (current_idx + 1) % len(self.image_files)
        prev_idx = (current_idx - 1) % len(self.image_files)
        
        # Preload in priority order: next, previous
        for idx in [next_idx, prev_idx]:
            if self.stop_preloading.is_set():
                break
                
            if idx == current_idx:
                continue
                
            try:
                image_path = self.image_files[idx]
                
                # Check if already cached
                with self.preload_lock:
                    if str(image_path) in self.preload_cache:
                        continue
                
                # Load preview image
                with Image.open(image_path) as img:
                    # Create thumbnail for faster loading
                    max_size = 1024
                    if max(img.size) > max_size:
                        ratio = max_size / max(img.size)
                        preview_size = (int(img.size[0] * ratio), int(img.size[1] * ratio))
                        img.thumbnail(preview_size, Image.Resampling.BILINEAR)
                    
                    if img.mode != 'RGB':
                        img = img.convert('RGB')
                    
                    preview_image = img.copy()
                    
                    # Store in preload cache
                    with self.preload_lock:
                        self.preload_cache[str(image_path)] = {
                            'image': preview_image,
                            'size': img.size,
                            'mode': img.mode
                        }
                        
                        # Limit preload cache size
                        if len(self.preload_cache) > 10:
                            # Remove oldest entry
                            oldest_key = next(iter(self.preload_cache))
                            del self.preload_cache[oldest_key]
                            
            except Exception:
                # Silently ignore preload errors
                pass
    
    def update_status(self):
        """Update status bar"""
        if not self.image_files:
            return
        
        current_file = self.image_files[self.current_index].name
        total_zoom = self.base_zoom * self.zoom_factor
        status_text = f"{self.current_index + 1}/{len(self.image_files)} - {current_file} - Zoom: {total_zoom:.2f}x"
        
        # Show composition info if enabled
        if self.show_composition:
            composition_name = self.composition_types[self.composition_index].replace('_', ' ').title()
            status_text += f" - Composición: {composition_name}"
        
        # Show bookmarks info
        if self.bookmarks:
            bookmarks_list = sorted(self.bookmarks.keys())
            status_text += f" - Bookmarks: {','.join(map(str, bookmarks_list))}"
        
        # Show temporary bookmark message if any
        if self.bookmark_message:
            status_text += f" - {self.bookmark_message}"
        
        self.status_label.config(text=status_text)
    
    def on_key_press(self, event):
        """Handle keyboard events"""
        if event.keysym == 'Left':
            self.previous_image()
        elif event.keysym == 'Right':
            self.next_image()
        elif event.keysym.lower() == 'l':
            self.like_image()
        elif event.keysym.lower() == 'w':
            self.worse_image()
        elif event.keysym.lower() == 'u':
            self.undo_last_move()
        elif event.keysym.lower() == 'c':
            self.toggle_composition_lines()
        elif event.keysym == 'Up':
            self.next_composition()
        elif event.keysym == 'Down':
            self.prev_composition()
        elif event.keysym.isdigit():
            digit = int(event.keysym)
            if digit == 0:
                self.clear_all_bookmarks()
            elif 1 <= digit <= 9:
                self.handle_bookmark(digit)
        elif event.keysym == 'Escape':
            self.safe_exit()
    
    def previous_image(self):
        """Go to previous image"""
        if self.image_files:
            self.current_index = (self.current_index - 1) % len(self.image_files)
            self.load_current_image()
    
    def next_image(self):
        """Go to next image"""
        if self.image_files:
            self.current_index = (self.current_index + 1) % len(self.image_files)
            self.load_current_image()
    
    def like_image(self):
        """Move current image to best directory"""
        if not self.image_files:
            return
        
        try:
            current_file = self.image_files[self.current_index]
            destination = self.best_dir / current_file.name
            
            # Move file
            shutil.move(str(current_file), str(destination))
            
            # Track for undo
            self.last_moved = (current_file, destination, 'best')
            
            # Remove from current list
            self.image_files.pop(self.current_index)
            
            # Adjust index if necessary
            if self.current_index >= len(self.image_files):
                self.current_index = 0
            
            if self.image_files:
                self.load_current_image()
            else:
                self.set_status_message("Todas las imágenes procesadas", 5000)
                self.root.after(5000, self.safe_exit)
                
        except Exception as e:
            self.set_status_message(f"Error al mover imagen: {str(e)[:50]}...", 5000)
    
    def worse_image(self):
        """Move current image to worse directory"""
        if not self.image_files:
            return
            
        try:
            current_file = self.image_files[self.current_index]
            dest_file = self.worse_dir / current_file.name
            
            # Move the image
            shutil.move(str(current_file), str(dest_file))
            
            # Track for undo
            self.last_moved = (current_file, dest_file, 'worse')
            
            # Remove from image list
            self.image_files.pop(self.current_index)
            
            # Clear bookmarks that reference moved or subsequent images
            self.clear_affected_bookmarks()
            
            # Update status message
            self.set_status_message(f"Movido a worse/: {current_file.name}")
            
            # Adjust index if necessary
            if self.current_index >= len(self.image_files):
                self.current_index = 0
            
            if self.image_files:
                self.load_current_image()
            else:
                self.set_status_message("Todas las imágenes procesadas", 5000)
                self.root.after(5000, self.safe_exit)
                
        except Exception as e:
            self.set_status_message(f"Error al mover a worse/: {str(e)[:50]}...", 5000)
    
    def clear_affected_bookmarks(self):
        """Clear bookmarks affected by image removal"""
        if not self.bookmarks:
            return
            
        # Get current index before removal
        removed_index = self.current_index
        bookmarks_to_remove = []
        
        # Check each bookmark
        for number, state in self.bookmarks.items():
            bookmark_index = state[0]  # First element is the index
            
            if bookmark_index == removed_index:
                # Bookmark points to removed image
                bookmarks_to_remove.append(number)
            elif bookmark_index > removed_index:
                # Adjust bookmark index for images that shifted down
                adjusted_state = list(state)
                adjusted_state[0] = bookmark_index - 1
                self.bookmarks[number] = tuple(adjusted_state)
        
        # Remove invalid bookmarks
        for number in bookmarks_to_remove:
            del self.bookmarks[number]
        
        if bookmarks_to_remove:
            removed_str = ','.join(map(str, bookmarks_to_remove))
            self.set_status_message(f"Bookmarks {removed_str} eliminados", 3000)
    
    def setup_signal_handlers(self):
        """Setup signal handlers for graceful exit"""
        def signal_handler(signum, frame):
            self.safe_exit()
        
        # Handle Ctrl+C (SIGINT) and SIGTERM
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)
        
        # Handle window close event
        self.root.protocol("WM_DELETE_WINDOW", self.safe_exit)
    
    def safe_exit(self):
        """Safely exit the application"""
        try:
            # Stop background threads
            if hasattr(self, 'stop_preloading'):
                self.stop_preloading.set()
            
            # Cancel any pending timeouts
            if hasattr(self, 'message_timeout') and self.message_timeout:
                self.root.after_cancel(self.message_timeout)
            
            # Clear any temporary data and caches
            if hasattr(self, 'composition_cache'):
                self.composition_cache.clear()
            if hasattr(self, 'image_cache'):
                self.image_cache.clear()
            if hasattr(self, 'metadata_cache'):
                self.metadata_cache.clear()
            if hasattr(self, 'preload_cache'):
                with self.preload_lock:
                    self.preload_cache.clear()
                
            # Destroy the window and exit
            self.root.destroy()
            sys.exit(0)
            
        except Exception:
            # Force exit if something goes wrong
            sys.exit(0)
    
    def extract_metadata(self, image_path):
        """Extract comprehensive metadata from image - cached"""
        # Check cache first
        path_str = str(image_path)
        mtime = image_path.stat().st_mtime
        cache_key = (path_str, mtime)
        
        cached_metadata = self.metadata_cache.get(cache_key)
        if cached_metadata:
            return cached_metadata
        
        metadata = {}
        
        try:
            # File system metadata
            stat_info = image_path.stat()
            metadata['file'] = {
                'name': image_path.name,
                'size': self.format_file_size(stat_info.st_size),
                'created': datetime.fromtimestamp(stat_info.st_ctime).strftime('%Y-%m-%d %H:%M:%S'),
                'modified': datetime.fromtimestamp(stat_info.st_mtime).strftime('%Y-%m-%d %H:%M:%S'),
                'path': str(image_path.parent)
            }
            
            # Image metadata
            with Image.open(image_path) as img:
                metadata['image'] = {
                    'dimensions': f"{img.width} × {img.height}",
                    'mode': img.mode,
                    'format': img.format or 'Unknown',
                    'megapixels': f"{(img.width * img.height) / 1000000:.1f} MP"
                }
                
                # Calculate aspect ratio
                gcd = math.gcd(img.width, img.height)
                ratio_w = img.width // gcd
                ratio_h = img.height // gcd
                metadata['image']['aspect_ratio'] = f"{ratio_w}:{ratio_h}"
                
                # EXIF data
                if hasattr(img, '_getexif') and img._getexif():
                    exif_data = img._getexif()
                    metadata['exif'] = self.parse_exif_data(exif_data)
                else:
                    metadata['exif'] = {}
                    
        except Exception as e:
            metadata['error'] = f"Error extracting metadata: {str(e)}"
        
        # Cache the result using LRU cache
        self.metadata_cache.put(cache_key, metadata)
            
        return metadata
    
    def parse_exif_data(self, exif_data):
        """Parse EXIF data into readable format - optimized"""
        parsed = {}
        
        # Pre-defined important tags as class constant would be better, but this works
        important_tags = {
            'Make': 'Camera Make', 'Model': 'Camera Model',
            'DateTime': 'Date Taken', 'DateTimeOriginal': 'Original Date',
            'ExposureTime': 'Shutter Speed', 'FNumber': 'Aperture',
            'ISO': 'ISO', 'ISOSpeedRatings': 'ISO Speed',
            'FocalLength': 'Focal Length', 'Flash': 'Flash',
            'WhiteBalance': 'White Balance', 'ExposureMode': 'Exposure Mode',
            'MeteringMode': 'Metering Mode', 'ColorSpace': 'Color Space',
            'ExifImageWidth': 'EXIF Width', 'ExifImageHeight': 'EXIF Height',
            'Orientation': 'Orientation', 'XResolution': 'X Resolution',
            'YResolution': 'Y Resolution', 'Software': 'Software',
            'Artist': 'Artist', 'Copyright': 'Copyright',
            'ImageDescription': 'Description', 'LensModel': 'Lens Model',
            'LensMake': 'Lens Make', 'GPSInfo': 'GPS Info'
        }
        
        for tag_id, value in exif_data.items():
            tag_name = TAGS.get(tag_id, tag_id)
            
            if tag_name in important_tags:
                display_name = important_tags[tag_name]
                
                # Special formatting for certain values
                if tag_name == 'ExposureTime':
                    parsed[display_name] = self.format_shutter_speed(value)
                        
                elif tag_name == 'FNumber':
                    if isinstance(value, tuple) and len(value) == 2 and value[1] != 0:
                        f_stop = value[0] / value[1]
                        parsed[display_name] = f"f/{f_stop:.1f}"
                    else:
                        parsed[display_name] = str(value)
                        
                elif tag_name == 'FocalLength':
                    if isinstance(value, tuple) and len(value) == 2 and value[1] != 0:
                        focal = value[0] / value[1]
                        parsed[display_name] = f"{focal:.0f}mm"
                    else:
                        parsed[display_name] = str(value)
                        
                elif tag_name == 'Flash':
                    parsed[display_name] = FLASH_MODES.get(value, f"Unknown ({value})")
                    
                elif tag_name == 'GPSInfo' and isinstance(value, dict):
                    gps_info = self.parse_gps_data(value)
                    if gps_info:
                        parsed['GPS Location'] = gps_info
                        
                else:
                    parsed[display_name] = str(value)
                    
        return parsed
    
    def parse_gps_data(self, gps_data):
        """Parse GPS data from EXIF"""
        try:
            if 2 in gps_data and 4 in gps_data:  # Latitude and Longitude
                lat_ref = gps_data.get(1, 'N')
                lat_data = gps_data[2]
                lon_ref = gps_data.get(3, 'E')
                lon_data = gps_data[4]
                
                # Convert to decimal degrees
                lat = self.dms_to_decimal(lat_data, lat_ref)
                lon = self.dms_to_decimal(lon_data, lon_ref)
                
                return f"{lat:.6f}°, {lon:.6f}°"
        except:
            pass
        return None
    
    def dms_to_decimal(self, dms, ref):
        """Convert degrees, minutes, seconds to decimal"""
        degrees = float(dms[0])
        minutes = float(dms[1]) / 60.0
        seconds = float(dms[2]) / 3600.0
        
        decimal = degrees + minutes + seconds
        
        if ref in ['S', 'W']:
            decimal = -decimal
            
        return decimal
    
    def format_file_size(self, size_bytes):
        """Format file size in human readable format"""
        if size_bytes < 1024:
            return f"{size_bytes} B"
        elif size_bytes < 1024**2:
            return f"{size_bytes/1024:.1f} KB"
        elif size_bytes < 1024**3:
            return f"{size_bytes/(1024**2):.1f} MB"
        else:
            return f"{size_bytes/(1024**3):.1f} GB"
    
    def update_metadata(self):
        """Update metadata display in sidebar"""
        if not hasattr(self, 'image_files') or not self.image_files:
            return
            
        # Clear existing metadata display
        for widget in self.scrollable_frame.winfo_children():
            widget.destroy()
            
        # Extract metadata for current image
        current_image_path = self.image_files[self.current_index]
        metadata = self.extract_metadata(current_image_path)
        
        # Display metadata sections
        self.display_metadata_section("ARCHIVO", metadata.get('file', {}))
        self.display_metadata_section("IMAGEN", metadata.get('image', {}))
        
        if metadata.get('exif'):
            self.display_metadata_section("EXIF", metadata['exif'])
            
        # Show focus if calculated
        if hasattr(self, 'focus_percentage') and self.focus_percentage is not None:
            focus_data = {
                'Porcentaje': f"{self.focus_percentage:.1f}%",
                'Calidad': self.get_focus_quality(self.focus_percentage)
            }
            self.display_metadata_section("ENFOQUE", focus_data)
            
        # Show error if any
        if 'error' in metadata:
            error_label = tk.Label(
                self.scrollable_frame,
                text=metadata['error'],
                bg='gray25',
                fg='red',
                font=('Arial', 9),
                wraplength=280,
                justify=tk.LEFT
            )
            error_label.pack(fill=tk.X, padx=10, pady=5)
    
    def display_metadata_section(self, title, data):
        """Display a metadata section"""
        if not data:
            return
            
        # Section title
        title_frame = tk.Frame(self.scrollable_frame, bg='gray30', height=25)
        title_frame.pack(fill=tk.X, padx=5, pady=(10, 0))
        title_frame.pack_propagate(False)
        
        title_label = tk.Label(
            title_frame,
            text=title,
            bg='gray30',
            fg='lightblue',
            font=('Arial', 10, 'bold'),
            anchor='w'
        )
        title_label.pack(fill=tk.BOTH, padx=8, pady=3)
        
        # Data items
        for key, value in data.items():
            self.create_metadata_item(key, value)
            
    def create_metadata_item(self, label, value):
        """Create a metadata item display"""
        item_frame = tk.Frame(self.scrollable_frame, bg='gray25')
        item_frame.pack(fill=tk.X, padx=10, pady=1)
        
        # Label
        label_widget = tk.Label(
            item_frame,
            text=f"{label}:",
            bg='gray25',
            fg='lightgray',
            font=('Arial', 9),
            anchor='w',
            width=12
        )
        label_widget.pack(side=tk.LEFT, anchor='nw')
        
        # Value (with text wrapping for long values)
        value_str = str(value)
        if len(value_str) > 30:
            # Multi-line for long values
            value_widget = tk.Label(
                item_frame,
                text=value_str,
                bg='gray25',
                fg='white',
                font=('Arial', 9),
                anchor='w',
                justify=tk.LEFT,
                wraplength=180
            )
        else:
            # Single line for short values
            value_widget = tk.Label(
                item_frame,
                text=value_str,
                bg='gray25',
                fg='white',
                font=('Arial', 9),
                anchor='w'
            )
        
        value_widget.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(5, 0))
    
    def get_focus_quality(self, focus_percentage):
        """Get focus quality description"""
        if focus_percentage < 30:
            return "Desenfocado"
        elif focus_percentage < 70:
            return "Parcial"
        else:
            return "Bien enfocado"
    
    def format_shutter_speed(self, value):
        """Format shutter speed maintaining fractional representation"""
        try:
            # Handle PIL.TiffImagePluginIFDRational objects
            if hasattr(value, 'numerator') and hasattr(value, 'denominator'):
                numerator = value.numerator
                denominator = value.denominator
                
                # Handle zero denominator
                if denominator == 0:
                    return f"Invalid: {value}"
                
                # Simplify the fraction
                gcd = math.gcd(abs(numerator), abs(denominator))
                simplified_num = numerator // gcd
                simplified_den = denominator // gcd
                
                # Format based on the simplified fraction
                if simplified_num == 1:
                    return f"1/{simplified_den}s"
                elif simplified_den == 1:
                    return f"{simplified_num}s"
                elif simplified_num > simplified_den:
                    # For long exposures, show as decimal if it's cleaner
                    decimal_value = simplified_num / simplified_den
                    if decimal_value == int(decimal_value):
                        return f"{int(decimal_value)}s"
                    else:
                        return f"{decimal_value:.1f}s"
                else:
                    return f"{simplified_num}/{simplified_den}s"
                    
            elif isinstance(value, tuple) and len(value) == 2:
                numerator, denominator = value
                
                # Handle zero denominator
                if denominator == 0:
                    return f"Invalid: {value}"
                
                # Keep original fraction values as integers to preserve precision
                num = int(numerator)
                den = int(denominator)
                
                # Simplify the fraction
                gcd = math.gcd(abs(num), abs(den))
                simplified_num = num // gcd
                simplified_den = den // gcd
                
                # Format based on the simplified fraction
                if simplified_num == 1:
                    return f"1/{simplified_den}s"
                elif simplified_den == 1:
                    return f"{simplified_num}s"
                elif simplified_num > simplified_den:
                    # For long exposures, show as decimal if it's cleaner
                    decimal_value = simplified_num / simplified_den
                    if decimal_value == int(decimal_value):
                        return f"{int(decimal_value)}s"
                    else:
                        return f"{decimal_value:.1f}s"
                else:
                    return f"{simplified_num}/{simplified_den}s"
                    
            elif isinstance(value, (int, float)):                
                # Handle direct numeric values
                if value >= 1:
                    if value == int(value):
                        return f"{int(value)}s"
                    else:
                        return f"{value:.1f}s"
                else:
                    # Convert decimal to fraction representation using precomputed values
                    # Find closest match
                    for speed_value, speed_text in COMMON_SHUTTER_SPEEDS:
                        if abs(value - speed_value) < 0.0001:  # Small tolerance for floating point
                            return f"{speed_text}s"
                    
                    # If no exact match, create fraction
                    if value < 1:
                        denominator = int(round(1/value))
                        return f"1/{denominator}s"
                    else:
                        return f"{value:.1f}s"
            else:
                return str(value)
                
        except Exception as e:
            return f"Error: {str(e)}"
    
    def toggle_composition_lines(self):
        """Toggle composition lines on/off"""
        self.show_composition = not self.show_composition
        self.update_display()
    
    def next_composition(self):
        """Switch to next composition type"""
        if self.show_composition:
            self.composition_index = (self.composition_index + 1) % len(self.composition_types)
            self.update_display()
    
    def prev_composition(self):
        """Switch to previous composition type"""
        if self.show_composition:
            self.composition_index = (self.composition_index - 1) % len(self.composition_types)
            self.update_display()
    
    def save_current_state(self):
        """Save current image state - optimized"""
        return (
            self.current_index,
            self.zoom_factor,
            self.base_zoom,
            self.image_x,
            self.image_y,
            self.show_composition,
            self.composition_index
        )
    
    def restore_state(self, state):
        """Restore image state - optimized"""
        if not self.image_files or state[0] >= len(self.image_files):
            return False
            
        # Change to the bookmarked image
        self.current_index = state[0]
        self.load_current_image()
        
        # Wait for image to load, then restore zoom and position
        self.root.after(30, lambda: self.restore_view_state(state))
        return True
    
    def restore_view_state(self, state):
        """Restore zoom, position and composition - optimized"""
        # Unpack state tuple for better performance
        _, zoom, base_zoom, x, y, composition, comp_type = state
        
        # Restore all state at once
        self.zoom_factor = zoom
        self.base_zoom = base_zoom
        self.image_x, self.image_y = x, y
        self.show_composition = composition
        self.composition_index = comp_type
        
        # Single update
        self.update_display()
        self.update_status()
    
    def set_status_message(self, message, duration=2000):
        """Set temporary message in status bar"""
        self.bookmark_message = message
        self.update_status()
        
        # Clear previous timeout
        if self.message_timeout:
            self.root.after_cancel(self.message_timeout)
        
        # Set new timeout to clear message
        self.message_timeout = self.root.after(duration, self.clear_status_message)
    
    def clear_status_message(self):
        """Clear temporary message from status bar"""
        self.bookmark_message = ""
        self.message_timeout = None
        self.update_status()
    
    def handle_bookmark(self, number):
        """Handle bookmark key press (1-9) - optimized"""
        if number in self.bookmarks:
            # Restore bookmarked state
            success = self.restore_state(self.bookmarks[number])
            if success:
                self.set_status_message(f"Restaurado bookmark {number}")
            else:
                # Invalid bookmark, remove it
                del self.bookmarks[number]
                self.set_status_message(f"Bookmark {number} eliminado (inválido)", 3000)
        else:
            # Save current state as bookmark
            self.bookmarks[number] = self.save_current_state()
            filename = self.image_files[self.current_index].name
            self.set_status_message(f"Guardado bookmark {number}: {filename[:20]}...")
    
    def clear_all_bookmarks(self):
        """Clear all bookmarks - optimized"""
        if self.bookmarks:
            count = len(self.bookmarks)
            self.bookmarks.clear()
            self.set_status_message(f"Eliminados {count} bookmark(s)")
        else:
            self.set_status_message("No hay bookmarks")
    
    def on_mouse_wheel(self, event):
        """Handle mouse wheel for zooming"""
        # Get mouse position relative to canvas
        mouse_x = event.x
        mouse_y = event.y
        
        # Calculate zoom - handle different platforms
        if (hasattr(event, 'num') and event.num == 4) or (hasattr(event, 'delta') and event.delta > 0):  # Scroll up
            zoom_change = 1.1
        elif (hasattr(event, 'num') and event.num == 5) or (hasattr(event, 'delta') and event.delta < 0):  # Scroll down
            zoom_change = 0.9
        else:
            return
        
        old_zoom = self.zoom_factor
        self.zoom_factor *= zoom_change
        
        # Limit zoom range
        self.zoom_factor = max(0.1, min(10.0, self.zoom_factor))
        
        # Adjust image position to zoom towards mouse cursor
        if old_zoom != self.zoom_factor:
            zoom_ratio = self.zoom_factor / old_zoom
            self.image_x = mouse_x - (mouse_x - self.image_x) * zoom_ratio
            self.image_y = mouse_y - (mouse_y - self.image_y) * zoom_ratio
            
            self.update_display()
            self.update_status()
    
    def on_mouse_click(self, event):
        """Handle mouse click for dragging - optimized"""
        self.drag_x = event.x
        self.drag_y = event.y
    
    def on_mouse_drag(self, event):
        """Handle mouse drag for panning - optimized"""
        dx = event.x - self.drag_x
        dy = event.y - self.drag_y
        
        self.image_x += dx
        self.image_y += dy
        self.drag_x = event.x
        self.drag_y = event.y
        
        # Direct canvas manipulation - fastest
        self.canvas.move("all", dx, dy)
    
    def on_mouse_release(self, event):
        """Handle mouse release"""
        pass
    
    def on_canvas_resize(self, event):
        """Handle canvas resize"""
        if hasattr(self, 'original_image'):
            # Only update if canvas size actually changed
            canvas_width = self.canvas.winfo_width()
            canvas_height = self.canvas.winfo_height()
            current_size = (canvas_width, canvas_height)
            
            if current_size != self.last_canvas_size and canvas_width > 1 and canvas_height > 1:
                self.last_canvas_size = current_size
                self.calculate_fit_to_window()
                self.image_cache.clear()  # Clear cache on resize
                self.update_display()
    
    def draw_composition_lines(self):
        """Draw composition lines based on current type"""
        if not hasattr(self, 'photo'):
            return
        
        # Get image bounds on canvas
        total_zoom = self.base_zoom * self.zoom_factor
        orig_width, orig_height = self.original_image.size
        img_width = int(orig_width * total_zoom)
        img_height = int(orig_height * total_zoom)
        
        # Calculate image position on canvas
        canvas_width = self.canvas.winfo_width()
        canvas_height = self.canvas.winfo_height()
        
        # Image position
        if img_width < canvas_width:
            img_x = (canvas_width - img_width) // 2
        else:
            img_x = self.image_x
            
        if img_height < canvas_height:
            img_y = (canvas_height - img_height) // 2
        else:
            img_y = self.image_y
        
        # Check if we can use cached composition lines with improved caching
        current_params = (self.composition_index, img_x, img_y, img_width, img_height, canvas_width, canvas_height)
        cache_key = f"{self.composition_types[self.composition_index]}_{img_width}x{img_height}_{img_x},{img_y}"
        
        if cache_key in self.composition_cache:
            cached_lines = self.composition_cache[cache_key]
            # Clear and redraw from cache
            self.canvas.delete("composition")
            for line_data in cached_lines:
                self.canvas.create_line(*line_data['coords'], 
                                      fill=line_data['fill'], 
                                      width=line_data['width'], 
                                      tags="composition")
            self.last_composition_params = current_params
            return
        
        # Clear previous composition lines
        self.canvas.delete("composition")
        
        composition_type = self.composition_types[self.composition_index]
        
        # Use lookup table for better performance
        composition_functions = {
            "rule_of_thirds": self.draw_rule_of_thirds_fast,
            "golden_ratio": self.draw_golden_ratio_fast,
            "diagonal": self.draw_diagonal_fast,
            "triangle": self.draw_triangle_fast,
            "golden_spiral": self.draw_golden_spiral_fast,
            "center_lines": self.draw_center_lines_fast,
            "phi_grid": self.draw_phi_grid_fast,
            "dynamic_symmetry": self.draw_dynamic_symmetry_fast,
            "armature_of_rectangle": self.draw_armature_of_rectangle_fast,
            "rabatment": self.draw_rabatment_fast,
            "harmonious_triangles": self.draw_harmonious_triangles_fast,
            "sinister_diagonal": self.draw_sinister_diagonal_fast,
            "aspect_ratio_guides": self.draw_aspect_ratio_guides_fast
        }
        
        # Call the appropriate function and cache the results
        func = composition_functions.get(composition_type)
        if func:
            lines_data = func(img_x, img_y, img_width, img_height)
            
            # Cache the line data
            if len(self.composition_cache) > 20:  # Limit cache size
                # Remove oldest entry
                oldest_key = next(iter(self.composition_cache))
                del self.composition_cache[oldest_key]
            
            self.composition_cache[cache_key] = lines_data
        
        # Cache current parameters
        self.last_composition_params = current_params
    
    # Optimized composition drawing functions
    def draw_rule_of_thirds_fast(self, x, y, w, h):
        """Draw rule of thirds grid - optimized with caching"""
        w3, w23 = x + w/3, x + 2*w/3
        h3, h23 = y + h/3, y + 2*h/3
        
        lines = [
            (w3, y, w3, y + h),      # Vertical 1/3
            (w23, y, w23, y + h),    # Vertical 2/3
            (x, h3, x + w, h3),      # Horizontal 1/3
            (x, h23, x + w, h23)     # Horizontal 2/3
        ]
        
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        
        return lines_data
    
    def draw_golden_ratio_fast(self, x, y, w, h):
        """Draw golden ratio lines - optimized with caching"""
        v1, v2 = x + w * PHI_INV, x + w * (1 - PHI_INV)
        h1, h2 = y + h * PHI_INV, y + h * (1 - PHI_INV)
        
        lines = [
            (v1, y, v1, y + h),      # Vertical golden
            (v2, y, v2, y + h),      # Vertical golden complement
            (x, h1, x + w, h1),      # Horizontal golden
            (x, h2, x + w, h2)       # Horizontal golden complement
        ]
        
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        
        return lines_data
    
    def draw_diagonal_fast(self, x, y, w, h):
        """Draw diagonal lines - optimized with caching"""
        lines = [
            (x, y, x + w, y + h),        # Main diagonal
            (x + w, y, x, y + h)         # Anti-diagonal
        ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_triangle_fast(self, x, y, w, h):
        """Draw triangular composition lines - optimized with caching"""
        mid_x = x + w/2
        lines = [
            (mid_x, y, x, y + h),        # Left triangle side
            (mid_x, y, x + w, y + h),    # Right triangle side
            (x, y + h, x + w, y + h)     # Base
        ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_golden_spiral_fast(self, x, y, w, h):
        """Draw golden spiral guidelines - optimized with caching"""
        w1, h1 = x + w * PHI_INV, y + h * PHI_INV
        lines = [
            (w1, y, w1, y + h),          # Main vertical
            (x, h1, x + w, h1),          # Main horizontal
            (w1, h1, x + w, h1),         # Sub horizontal
            (w1, h1, w1, y + h)          # Sub vertical
        ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_center_lines_fast(self, x, y, w, h):
        """Draw center cross lines - optimized with caching"""
        mid_x, mid_y = x + w/2, y + h/2
        lines = [
            (mid_x, y, mid_x, y + h),    # Vertical center
            (x, mid_y, x + w, mid_y)     # Horizontal center
        ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_phi_grid_fast(self, x, y, w, h):
        """Draw phi-based grid - optimized with caching"""
        # Precomputed positions for speed - could be module level constant
        positions = (0.276, 0.447, PHI_INV)  # i/(phi+2) for i=1,2,3
        lines = []
        for pos in positions:
            lines.extend([
                (x + w * pos, y, x + w * pos, y + h),  # Vertical
                (x, y + h * pos, x + w, y + h * pos)   # Horizontal
            ])
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=1, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 1
            })
        return lines_data
    
    def draw_dynamic_symmetry_fast(self, x, y, w, h):
        """Draw dynamic symmetry lines - optimized with caching"""
        mid_x, mid_y = x + w/2, y + h/2
        lines = [
            (x, y, x + w, y + h),        # Main diagonal
            (x + w, y, x, y + h),        # Anti-diagonal
            (x, mid_y, x + w, mid_y),    # Horizontal center
            (mid_x, y, mid_x, y + h)     # Vertical center
        ]
        lines_data = []
        for i, line in enumerate(lines):
            width = 2 if i < 2 else 1
            self.canvas.create_line(*line, fill="#FFFFFF", width=width, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': width
            })
        return lines_data
    
    def draw_armature_of_rectangle_fast(self, x, y, w, h):
        """Draw armature of rectangle - optimized with caching"""
        mid_x, mid_y = x + w/2, y + h/2
        lines = [
            (x, y, x + w, y + h),        # Main diagonal
            (x + w, y, x, y + h),        # Anti-diagonal  
            (mid_x, y, mid_x, y + h),    # Vertical center
            (x, mid_y, x + w, mid_y)     # Horizontal center
        ]
        lines_data = []
        for i, line in enumerate(lines):
            width = 2 if i < 2 else 1
            self.canvas.create_line(*line, fill="#FFFFFF", width=width, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': width
            })
        return lines_data
    
    def draw_rabatment_fast(self, x, y, w, h):
        """Draw rabatment of rectangle - optimized with caching"""
        if w > h:  # Landscape
            lines = [
                (x + h, y, x + h, y + h),        # Left square
                (x + w - h, y, x + w - h, y + h) # Right square
            ]
        else:  # Portrait
            lines = [
                (x, y + w, x + w, y + w),        # Top square
                (x, y + h - w, x + w, y + h - w) # Bottom square
            ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_harmonious_triangles_fast(self, x, y, w, h):
        """Draw harmonious triangles - optimized with caching"""
        mid_x, mid_y = x + w/2, y + h/2
        lines = [
            (x, y, x + w, mid_y),        # Top-right triangle
            (x + w, mid_y, x, y + h),    # Bottom-left triangle
            (x, y + h, mid_x, y),        # Left-top triangle
            (mid_x, y, x + w, y + h)     # Right-bottom triangle
        ]
        lines_data = []
        for line in lines:
            self.canvas.create_line(*line, fill="#FFFFFF", width=2, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': 2
            })
        return lines_data
    
    def draw_sinister_diagonal_fast(self, x, y, w, h):
        """Draw sinister diagonal with reciprocals - optimized with caching"""
        w3, w23 = x + w/3, x + 2*w/3
        lines = [
            (x + w, y, x, y + h),        # Main sinister diagonal
            (x, y, w3, y + h),           # Left reciprocal
            (w23, y, x + w, y + h)       # Right reciprocal
        ]
        lines_data = []
        for i, line in enumerate(lines):
            width = 2 if i == 0 else 1
            self.canvas.create_line(*line, fill="#FFFFFF", width=width, tags="composition")
            lines_data.append({
                'coords': line,
                'fill': "#FFFFFF", 
                'width': width
            })
        return lines_data
    
    def draw_aspect_ratio_guides_fast(self, x, y, w, h):
        """Draw aspect ratio guides - optimized with caching"""
        ratio = w / h
        lines_data = []
        
        if abs(ratio - 16/9) < 0.1:  # Cinematic
            safe_margin = 0.05
            safe_x1, safe_y1 = x + w * safe_margin, y + h * safe_margin
            safe_x2, safe_y2 = x + w * (1 - safe_margin), y + h * (1 - safe_margin)
            self.canvas.create_rectangle(safe_x1, safe_y1, safe_x2, safe_y2,
                                       outline="#FFFFFF", width=1, tags="composition")
            # Store rectangle as lines for caching
            lines_data.append({
                'coords': (safe_x1, safe_y1, safe_x2, safe_y1),  # Top
                'fill': "#FFFFFF", 
                'width': 1
            })
            lines_data.append({
                'coords': (safe_x2, safe_y1, safe_x2, safe_y2),  # Right
                'fill': "#FFFFFF", 
                'width': 1
            })
            lines_data.append({
                'coords': (safe_x2, safe_y2, safe_x1, safe_y2),  # Bottom
                'fill': "#FFFFFF", 
                'width': 1
            })
            lines_data.append({
                'coords': (safe_x1, safe_y2, safe_x1, safe_y1),  # Left
                'fill': "#FFFFFF", 
                'width': 1
            })
        else:  # Generic guides
            w4, w34 = x + w/4, x + 3*w/4
            lines = [
                (w4, y, w4, y + h),      # Quarter line left
                (w34, y, w34, y + h)     # Quarter line right
            ]
            for line in lines:
                self.canvas.create_line(*line, fill="#FFFFFF", width=1, tags="composition")
                lines_data.append({
                    'coords': line,
                    'fill': "#FFFFFF", 
                    'width': 1
                })
        
        return lines_data
    
    def undo_last_move(self):
        """Undo the last image move operation - optimized"""
        if not self.last_moved:
            self.set_status_message("No hay imagen para deshacer")
            return
        
        source_path, dest_path, dest_dir_type = self.last_moved
        
        try:
            # Usar objetos Path directamente (ya son Path objects)
            if not dest_path.exists():
                self.set_status_message("La imagen ya no existe en el directorio destino")
                self.last_moved = None
                return
            
            # Mover imagen de vuelta (rename es más eficiente que shutil.move)
            dest_path.rename(source_path)
            
            # Solo actualizar si es la carpeta actual - optimizado
            current_dir_path = Path(self.current_dir)
            if source_path.parent == current_dir_path:
                # Insertar directamente en lugar de reconstruir toda la lista
                insert_pos = 0
                for i, existing_file in enumerate(self.image_files):
                    if existing_file.name > source_path.name:
                        insert_pos = i
                        break
                    insert_pos = i + 1
                
                self.image_files.insert(insert_pos, source_path)
                self.current_index = insert_pos
                
                # Actualizar display solo si es necesario
                self.load_current_image()
            
            self.set_status_message(f"Restaurada desde {dest_dir_type}")
            self.last_moved = None
            
        except Exception as e:
            self.set_status_message(f"Error: {str(e)[:30]}...")

def main():
    root = tk.Tk()
    app = ImageViewer(root)
    root.mainloop()

if __name__ == "__main__":
    main()